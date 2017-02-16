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
}
pub fn client() {
}
