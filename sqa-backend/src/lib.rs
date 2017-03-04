extern crate futures;
extern crate tokio_core;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate rmp_serde;
extern crate time;
extern crate uuid;
extern crate sqa_engine;
extern crate sqa_ffmpeg;

pub mod codec;
pub mod handlers;
pub mod actions;
pub mod state;

use handlers::Connection;
use state::Context;
use tokio_core::reactor::Core;
use tokio_core::net::UdpSocket;
pub fn main() {
    let mut core = Core::new().unwrap();
    let ctx = Context::new(core.remote());
    let hdl = core.handle();
    let addr = "127.0.0.1:1234".parse().unwrap();
    let sock = UdpSocket::bind(&addr, &hdl).unwrap();
    let conn = Connection::new(sock, core.remote(), ctx);
    println!("[+] SQA Backend is up & running!");
    core.run(conn).unwrap();
}
pub fn client() {
}
