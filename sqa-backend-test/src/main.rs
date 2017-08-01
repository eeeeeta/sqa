extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate tokio_core;
extern crate futures;
extern crate rmp_serde;
extern crate sqa_backend;

use tokio_core::reactor::{Timeout, Core, Handle};
use sqa_backend::codec::{SqaClientCodec, Command, Reply};
use tokio_core::net::{UdpFramed, UdpSocket};
use futures::{Future, Async, Poll, Sink, Stream};
use std::time::Duration;
use std::collections::VecDeque;
use std::fs;

#[derive(Serialize, Deserialize)]
pub struct TestData {
    replies: VecDeque<Reply>,
    commands: VecDeque<Command>,
}
pub struct Test<'a> {
    sock: &'a mut UdpFramed<SqaClientCodec>,
    data: TestData,
    timeout: Timeout,
    hdl: Handle
}
impl<'a> Future for Test<'a> {
    type Item = ();
    type Error = ::std::io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Async::Ready(_) = self.timeout.poll()? {
            self.timeout = Timeout::new(Duration::from_millis(100), &self.hdl)?;
            if let Some(cmd) = self.data.commands.pop_front() {
                println!("--> {:?}", cmd);
                self.sock.start_send(cmd)?;
            }
        }
        if let Async::Ready(msg) = self.sock.poll()? {
            let msg = msg.unwrap().unwrap();
            if let Some(rpl) = self.data.replies.pop_front() {
                println!("<-- {:?}", rpl);
                let _rpl = rmp_serde::to_vec(&rpl).unwrap();
                let _msg = rmp_serde::to_vec(&msg).unwrap();
                if _rpl != _msg {
                    panic!("Expected {:?}, got {:?}", rpl, msg);
                }
            }
        }
        if self.data.commands.len() == 0 && self.data.replies.len() == 0 {
            Ok(Async::Ready(()))
        }
        else {
            self.sock.poll_complete()?;
            Ok(Async::NotReady)
        }
    }
}
fn main() {
    let core = Core::new().unwrap();
    let addr = "127.0.0.1:1234".parse().unwrap();
    let recv_addr = "127.0.0.1:53001".parse().unwrap();
    let codec = SqaClientCodec::new(addr);
    let sock = UdpSocket::bind(&recv_addr, &core.handle()).unwrap();
    let mut sock = sock.framed(codec);
    for file in fs::read_dir("./tests").unwrap() {
        let file = file.unwrap();
    }
}
