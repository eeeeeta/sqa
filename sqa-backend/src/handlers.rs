use codec::{Command, RecvMessage, SendMessage, SendMessageExt, SqaWireCodec};
use futures::stream::Stream;
use futures::sink::Sink;
use futures::{Poll, Async, Future};
use futures::sync::mpsc::{self, Sender, Receiver};
use tokio_core::net::{UdpFramed, UdpSocket};
use tokio_core::reactor::Remote;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use time::{Duration, SteadyTime};
use serde::Serialize;
use serde_json;
use rosc::{OscType, OscMessage};
use codec::Reply;
use errors::*;
use std::io::Result as IoResult;

pub const INTERNAL_BUFFER_SIZE: usize = 128;

#[derive(Debug)]
pub struct Party {
    addr: SocketAddr,
    subscribed_at: SteadyTime,
}
pub trait ConnHandler {
    type Message;
    fn internal(&mut self, d: &mut ConnData<Self::Message>, m: Self::Message);
    fn external(&mut self, d: &mut ConnData<Self::Message>, p: Command) -> BackendResult<()>;
    fn init(&mut self, d: &mut ConnData<Self::Message>);
}
/*struct WaitingHandler {
    pkt: u32,
    time: SteadyTime,
    sender: oneshot::Sender<bool>
}*/
pub struct ConnData<M> {
    pub framed: UdpFramed<SqaWireCodec>,
    pub internal_rx: Receiver<M>,
    pub internal_tx: Sender<M>,
    pub parties: Vec<Party>,
    pub remote: Remote,
    pub addr: SocketAddr,
    pub path: String
}
impl<M> ConnData<M> {
    pub fn send_raw(&mut self, msg: SendMessage) -> IoResult<()> {
        self.framed.start_send(msg)?;
        Ok(())
    }
    pub fn respond<T: Into<OscMessage>>(&mut self, msg: T) -> IoResult<()> {
        self.framed.start_send(self.addr.msg_to(msg.into()))?;
        Ok(())
    }
    pub fn reply<T>(&mut self, data: T) -> IoResult<()> where T: Serialize {
        let j = serde_json::to_string(&data).unwrap(); // FIXME FIXME FIXME
        let mut path = String::from("/reply");
        path.push_str(&self.path);
        self.framed.start_send(self.addr.msg_to(OscMessage {
            addr: path,
            args: Some(vec![OscType::String(j)])
        }))?;
        Ok(())
    }
    pub fn subscribe(&mut self) {
        let a = self.addr.clone();
        self.parties.retain(|party| {
            party.addr != a
        });
        self.parties.push(Party {
            addr: a,
            subscribed_at: SteadyTime::now()
        });
    }
/*    pub fn register_interest(&mut self) -> IoResult<oneshot::Receiver<bool>> {
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
}*/
    pub fn broadcast<T: Into<OscMessage>>(&mut self, pdata: T) -> IoResult<usize> {
        let mut n_sent = 0;
        let now = SteadyTime::now();
        self.parties.retain(|party| {
            now - party.subscribed_at <= Duration::seconds(30)
        });
        let data = pdata.into();
        for party in self.parties.iter_mut() {
            self.framed.start_send(party.addr.msg_to(data.clone()))?;
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
    fn on_external(&mut self, addr: SocketAddr, pkt: BackendResult<(String, Command)>) -> BackendResult<()> {
        match pkt {
            Ok((path, pkt)) => {
                self.data.addr = addr;
                self.data.path = path;
                if let Err(e) = self.hdlr.external(&mut self.data, pkt) {
                    println!("ERROR in external handler: {:?}", e);
                }
                for party in self.data.parties.iter_mut() {
                    if party.addr == self.data.addr {
                        party.subscribed_at = SteadyTime::now();
                    }
                }
            },
            Err(e) => {
                self.data.framed.start_send(addr.msg_to(
                    Reply::DeserFailed { err: e.to_string() }.into()
                ))?;
                println!("Deser failed: {:?}", e);
            }
        };
        Ok(())
    }
}
impl<H> Future for Connection<H> where H: ConnHandler {
    type Item = ();
    type Error = ::std::io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        'outer: loop {
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
                        Ok(Async::NotReady) => break 'outer,
                        Err(e) => return Err(e)
                    }
                },
                Err(_) => {}
            }
        }
        self.data.framed.poll_complete()?;
        Ok(Async::NotReady)
    }
}
impl<H> Connection<H> where H: ConnHandler {
    pub fn new(socket: UdpSocket, remote: Remote, handler: H) -> Self {
        let framed = socket.framed(SqaWireCodec);
        let (tx, rx) = mpsc::channel::<H::Message>(INTERNAL_BUFFER_SIZE);
        let mut ret = Connection {
            data: ConnData {
                framed: framed,
                internal_tx: tx,
                internal_rx: rx,
                parties: vec![],
                remote: remote,
                addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
                path: String::new()
            },
            hdlr: handler
        };
        ret.hdlr.init(&mut ret.data);
        ret
    }
    pub fn get_internal_tx(&self) -> Sender<H::Message> {
        self.data.internal_tx.clone()
    }
}
