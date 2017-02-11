use codec::{Command, Packet, RecvMessage, SendMessage, SendMessageExt, SqaWireCodec};
use futures::stream::Stream;
use futures::sink::Sink;
use futures::{Poll, Async, Future};
use futures::sync::mpsc::{self, Sender, Receiver};
use tokio_core::net::{UdpFramed, UdpSocket};
use std::net::SocketAddr;
use time::{Duration, SteadyTime};
use std::io::Result as IoResult;

pub const VERSION: &str = "SQA Backend alpha";
pub const INTERNAL_BUFFER_SIZE: usize = 128;

#[derive(Debug)]
pub struct Party {
    addr: SocketAddr,
    subscribed_at: SteadyTime,
    their_id: u32,
    our_id: u32
}
pub enum ServerMessage {
}
pub trait ServerHandler {
    fn internal(&mut self, d: &mut ServerData, m: ServerMessage);
    fn oneshot(&mut self, d: &mut ServerData, a: SocketAddr, p: Packet);
    fn registered(&mut self, d: &mut ServerData, id: usize, c: Command);
}
pub struct ServerData {
    pub framed: UdpFramed<SqaWireCodec>,
    pub internal_rx: Receiver<ServerMessage>,
    pub internal_tx: Sender<ServerMessage>,
    pub parties: Vec<Party>,
    party_data: Option<(usize, u32)>
}
impl ServerData {
    pub fn send_raw(&mut self, msg: SendMessage) -> IoResult<()> {
        self.framed.start_send(msg)?;
        Ok(())
    }
    pub fn reply(&mut self, msg: Command) -> IoResult<bool> {
        if let Some((pid, reply_to)) = self.party_data {
            let party = self.parties.get_mut(pid)
                .expect("ServerData::reply(): party data somehow changed. this is a bug!");
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
            Ok(false) // FIXME not ideal
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
pub struct Server<H> where H: ServerHandler {
    hdlr: H,
    data: ServerData
}
impl<H> Server<H> where H: ServerHandler {
    fn on_external(&mut self, addr: SocketAddr, pkt: Result<Packet, ::rmp_serde::decode::Error>) -> Result<(), ::std::io::Error> {
        match pkt {
            Ok(pkt) => {
                let mut idx = None;
                for (i, party) in self.data.parties.iter_mut().enumerate() {
                    if addr == party.addr {
                        idx = Some(i);
                        party.their_id += 1;
                        if pkt.id != party.their_id {
                            self.data.framed.start_send(addr.cmd_to(Command::MessageSkipped))?;
                            party.their_id = pkt.id;
                        }
                    }
                }
                match idx {
                    Some(i) => {
                        self.data.party_data = Some((i, pkt.id));
                        if let Command::Ping = pkt.cmd {
                            self.data.reply(Command::Pong)?;
                        }
                        else {
                            self.hdlr.registered(&mut self.data, i, pkt.cmd);
                        }
                        self.data.party_data = None;
                    },
                    None => {
                        if let Command::Subscribe = pkt.cmd {
                            self.data.parties.push(Party {
                                addr: addr.clone(),
                                subscribed_at: SteadyTime::now(),
                                our_id: 0,
                                their_id: 0
                            });
                            self.data.framed.start_send(addr.cmd_to(Command::HelloClient {
                                    version: VERSION.to_string()
                                }))?;
                        }
                        else {
                            self.hdlr.oneshot(&mut self.data, addr, pkt)
                        }
                    }
                }
            },
            Err(e) => {
                println!("Deserialisation error from {}: {:?}", addr, e);
                self.data.framed.start_send(addr.cmd_to(Command::DeserializationFailed))?;
            }
        };
        Ok(())
    }
}
impl<T> Future for Server<T> where T: ServerHandler {
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
impl<T> Server<T> where T: ServerHandler {
    pub fn new(socket: UdpSocket, handler: T) -> Self {
        let framed = socket.framed(SqaWireCodec);
        let (tx, rx) = mpsc::channel::<ServerMessage>(INTERNAL_BUFFER_SIZE);
        Server {
            data: ServerData {
                framed: framed,
                internal_tx: tx,
                internal_rx: rx,
                parties: vec![],
                party_data: None
            },
            hdlr: handler
        }
    }
}
