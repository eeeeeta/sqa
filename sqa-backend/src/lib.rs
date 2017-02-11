extern crate futures;
extern crate tokio_core;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate rmp_serde;
extern crate time;

pub mod codec;
pub mod handlers;

use futures::{Future, Stream, Sink, Poll};
use futures::stream::SplitSink;
use tokio_core::reactor::Core;
use tokio_core::net::{UdpCodec, UdpFramed, UdpSocket};
use std::io::Result as SIoResult;
use std::net::SocketAddr;

use codec::{Command, Packet, RecvMessage, SendMessage, SqaWireCodec};

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
        Ok(())
    });
    l.run(server).unwrap();
}
struct Client {
    framed: UdpFramed<SqaWireCodec>
}
fn crappy_message_maker(dest: SocketAddr, cmd: Command) -> SendMessage {
    SendMessage {
        addr: dest,
        pkt: Packet {
            id: 0,
            reply_to: 0,
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
