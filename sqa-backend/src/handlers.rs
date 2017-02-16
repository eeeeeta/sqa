use codec::{Command, Packet, RecvMessage, SendMessage, SendMessageExt, SqaWireCodec};
use futures::stream::Stream;
use futures::sink::Sink;
use futures::{Poll, Async, Future};
use futures::sync::oneshot;
use futures::sync::mpsc::{self, Sender, Receiver};
use tokio_core::net::{UdpFramed, UdpSocket};
use std::net::SocketAddr;
use time::{Duration, SteadyTime};
use std::io::Result as IoResult;

pub const INTERNAL_BUFFER_SIZE: usize = 128;
pub const VERSION: &str = "SQA Backend alpha";

pub struct Party {
    addr: SocketAddr,
    subscribed_at: SteadyTime,
    their_id: u32,
    our_id: u32,
    waiting: Vec<WaitingHandler>
}
pub trait ConnHandler {
    type Message;
    fn internal(&mut self, d: &mut ConnData<Self::Message>, m: Self::Message);
    fn external(&mut self, d: &mut ConnData<Self::Message>, a: SocketAddr, p: Packet);
    fn registered(&mut self, d: &mut ConnData<Self::Message>, id: usize, c: Command);
    fn skipped(&mut self, d: &mut ConnData<Self::Message>, id: usize);
    fn deser_failed(&mut self, d: &mut ConnData<Self::Message>, a: SocketAddr, e: ::rmp_serde::decode::Error);
}
struct WaitingHandler {
    pkt: u32,
    time: SteadyTime,
    sender: oneshot::Sender<bool>
}
pub struct ConnData<M> {
    pub framed: UdpFramed<SqaWireCodec>,
    pub internal_rx: Receiver<M>,
    pub internal_tx: Sender<M>,
    pub parties: Vec<Party>,
    party_data: Option<(usize, u32)>
}
impl<M> ConnData<M> {
    pub fn send_raw(&mut self, msg: SendMessage) -> IoResult<()> {
        self.framed.start_send(msg)?;
        Ok(())
    }
    pub fn register_interest(&mut self) -> IoResult<oneshot::Receiver<bool>> {
        if let Some((pid, pkt)) = self.party_data {
            let party = self.parties.get_mut(pid)
                .expect("ConnData::register_interest(): party data somehow changed. this is a bug!");
            let (tx, rx) = oneshot::channel();
            let wait = WaitingHandler {
                pkt: pkt,
                time: SteadyTime::now(),
                sender: tx
            };
            party.waiting.push(wait);
            Ok(rx)
        }
        else {
            Err(::std::io::Error::new(::std::io::ErrorKind::Other, "API used incorrectly: calling register_interest() at the wrong time"))
        }
    }
    pub fn reply(&mut self, msg: Command) -> IoResult<bool> {
        if let Some((pid, reply_to)) = self.party_data {
            let party = self.parties.get_mut(pid)
                .expect("ConnData::reply(): party data somehow changed. this is a bug!");
            party.our_id += 1;
            let pkt = Packet {
                id: party.our_id,
                reply_to: reply_to,
                cmd: msg
            };
            self.framed.start_send(party.addr.pkt_to(pkt))?;
            Ok(true)
        }
        else {
            Err(::std::io::Error::new(::std::io::ErrorKind::Other, "API used incorrectly: calling reply() at the wrong time"))
        }
    }
    pub fn broadcast(&mut self, msg: Command) -> IoResult<usize> {
        let mut n_sent = 0;
        let now = SteadyTime::now();
        self.parties.retain(|party| {
            now - party.subscribed_at <= Duration::seconds(30)
        });
        for party in self.parties.iter_mut() {
            party.our_id += 1;
            let pkt = Packet {
                id: party.our_id,
                reply_to: 0,
                cmd: msg.clone()
            };
            self.framed.start_send(party.addr.pkt_to(pkt))?;
            n_sent += 1;
        }
        Ok(n_sent)
    }
}
pub struct Connection<H> where H: ConnHandler {
    hdlr: H,
    data: ConnData<H::Message>
}
impl<H> Connection<H> where H: ConnHandler {
    fn on_external(&mut self, addr: SocketAddr, pkt: Result<Packet, ::rmp_serde::decode::Error>) -> Result<(), ::std::io::Error> {
        match pkt {
            Ok(pkt) => {
                let mut idx = None;
                for (i, party) in self.data.parties.iter_mut().enumerate() {
                    if addr == party.addr {
                        idx = Some(i);
                        break;
                    }
                }
                match idx {
                    Some(i) => {
                        self.data.parties[i].their_id += 1;
                        if pkt.id != self.data.parties[i].their_id {
                            self.hdlr.skipped(&mut self.data, i);
                            self.data.framed.start_send(addr.cmd_to(Command::MessageSkipped))?;
                            if pkt.id > self.data.parties[i].their_id {
                                self.data.parties[i].their_id = pkt.id;
                            }
                        }
                        self.data.party_data = Some((i, pkt.id));
                        let mut idxs = vec![];
                        for (i, wait) in self.data.parties[i].waiting.iter().enumerate() {
                            if wait.pkt == pkt.id {
                                    idxs.push(i);
                            }
                        }
                        for i in idxs {
                            self.data.parties[i].waiting.remove(i).sender.complete(true);
                        }
                        if let Command::Ping = pkt.cmd {
                            self.data.reply(Command::Pong)?;
                        }
                        else {
                            self.hdlr.registered(&mut self.data, i, pkt.cmd);
                        }
                        self.data.party_data = None;
                    },
                    None => {
                        self.hdlr.external(&mut self.data, addr, pkt);
                    }
                }
            },
            Err(e) => {
                self.hdlr.deser_failed(&mut self.data, addr, e);
                self.data.framed.start_send(addr.cmd_to(Command::DeserializationFailed))?;
            }
        };
        Ok(())
    }
}
impl<H> Future for Connection<H> where H: ConnHandler {
    type Item = ();
    type Error = ::std::io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.data.internal_rx.poll() {
            Ok(Async::Ready(msg)) => {
                self.hdlr.internal(&mut self.data, msg.unwrap());
            },
            Ok(Async::NotReady) => {
                match self.data.framed.poll() {
                    Ok(Async::Ready(Some(RecvMessage { addr, pkt }))) => {
                        if let Err(e) = self.on_external(addr, pkt) {
                            println!("error in external handler: {:?}", e);
                        }
                    },
                    Ok(Async::Ready(None)) => unreachable!(),
                    Err(e) => return Err(e),
                    _ => {}
                }
            },
            Err(_) => {}
        }
        self.data.framed.poll_complete()?;
        Ok(Async::NotReady)
    }
}
impl<H> Connection<H> where H: ConnHandler {
    pub fn new(socket: UdpSocket, handler: H) -> Self {
        let framed = socket.framed(SqaWireCodec);
        let (tx, rx) = mpsc::channel::<H::Message>(INTERNAL_BUFFER_SIZE);
        Connection {
            data: ConnData {
                framed: framed,
                internal_tx: tx,
                internal_rx: rx,
                parties: vec![],
                party_data: None
            },
            hdlr: handler
        }
    }
    pub fn get_internal_tx(&self) -> Sender<H::Message> {
        self.data.internal_tx.clone()
    }
}
