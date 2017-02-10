extern crate futures;
extern crate tokio_core;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate rmp_serde;

use futures::{Future, Stream, Sink, Poll};
use futures::stream::SplitSink;
use tokio_core::reactor::Core;
use tokio_core::net::{UdpCodec, UdpFramed, UdpSocket};
use std::io::Result as SIoResult;
use std::net::SocketAddr;
use serde::{Deserialize, Serialize};
use rmp_serde::{Deserializer, Serializer};

#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    HelloServer { name: String },
    HelloClient { name: String, version: String },
    UnknownClient,
    Ping,
    Pong,
    DeserializationFailed,
    QueryMessageSkipped,
    MessageSkipped { id: u8 }
}
#[derive(Debug, Serialize, Deserialize)]
struct Packet {
    id: u32,
    cmd: Command
}
struct Message {
    addr: SocketAddr,
    pkt: Packet
}
struct SqaWireCodec;
impl UdpCodec for SqaWireCodec {
    type In = Result<Message, rmp_serde::decode::Error>;
    type Out = Message;
    fn decode(&mut self, src: &SocketAddr, buf: &[u8]) -> SIoResult<Self::In> {
        let mut decoder = Deserializer::new(buf);
        Ok(Deserialize::deserialize(&mut decoder).map(|x| Message {
            addr: src.clone(),
            pkt: x
        }))
    }
    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> SocketAddr {
        let _ = msg.pkt.serialize(&mut Serializer::new(buf));
        msg.addr
    }
}
struct Context {
    tx: SplitSink<UdpFramed<SqaWireCodec>>
}
use std::env;
pub fn main() {
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();
    let mut l = Core::new().unwrap();
    let handle = l.handle();
    let socket = UdpSocket::bind(&addr, &handle).unwrap();
    let (snk, src) = socket.framed(SqaWireCodec).split();
    println!("Listening on: {}", addr);
    let server = Context {
        tx: snk
    };
    let server = src.for_each(|msg| {
        match msg {
            Ok(Message { addr, pkt }) => {
                println!("From {}: {:?}", addr, pkt);
            },
            Err(err) => {
                println!("Deserialisation error: {:?}", err);
            }
        }
        Ok(())
    });
    l.run(server).unwrap();
}
struct Client {
    framed: UdpFramed<SqaWireCodec>
}
fn crappy_message_maker(dest: SocketAddr, cmd: Command) -> Message {
    Message {
        addr: dest,
        pkt: Packet {
            id: 0,
            cmd: cmd
        }
    }
}
pub fn client() {
    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8081".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();
    let dest = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let dest = dest.parse::<SocketAddr>().unwrap();
    let mut l = Core::new().unwrap();
    let handle = l.handle();
    println!("Binding to {}", addr);
    let socket = UdpSocket::bind(&addr, &handle).unwrap();
    let mut framed = socket.framed(SqaWireCodec);
    println!("Bound to {}", addr);
    println!("Attempting to communicate with {}", dest);
    framed.start_send(crappy_message_maker(dest, Command::Ping));
    l.run(framed.flush());
}
