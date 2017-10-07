#![recursion_limit = "1024"]
#![feature(slice_patterns, advanced_slice_patterns, try_from)]
extern crate futures;
extern crate tokio_core;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate rmp_serde;
extern crate time;
extern crate uuid;
extern crate sqa_engine;
extern crate sqa_ffmpeg;
extern crate rosc;
#[macro_use] extern crate error_chain;
extern crate chrono;
#[macro_use] extern crate sqa_osc_custom_derive;
extern crate url;
#[macro_use] extern crate log;
extern crate fern;
extern crate tokio_io;

#[macro_use]
pub mod action_manager;
pub mod codec;
pub mod handlers;
pub mod actions;
pub mod state;
pub mod errors;
pub mod mixer;
pub mod save;
pub mod undo;
pub mod waveform;

pub static VERSION: &str = env!("CARGO_PKG_VERSION");

use handlers::Connection;
use state::Context;
use tokio_core::reactor::Core;
use tokio_core::net::{TcpListener, UdpSocket};

mod jack {
    use sqa_engine::sqa_jack::handler::JackLoggingHandler;
    pub struct Logger;

    impl JackLoggingHandler for Logger {
        fn on_error(&mut self, msg: &str) {
            error!("{}", msg);
        }
        fn on_info(&mut self, msg: &str) {
            trace!("{}", msg);
        }
    }
}
pub fn main() {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!("[{}] {} {}", record.target(), record.level(), message))
        })
        .level(log::LogLevelFilter::Debug)
        .level_for("tokio_core", log::LogLevelFilter::Info)
        .level_for("mio", log::LogLevelFilter::Info)
        .chain(::std::io::stdout())
        .apply()
        .unwrap();
    info!("SQA Backend, version {}", VERSION);
    info!("an eta project <http://theta.eu.org>");
    info!("[+] Configuring JACK logging...");
    sqa_engine::sqa_jack::handler::set_logging_handler(jack::Logger);
    info!("[+] Initialising reactor...");
    let mut core = Core::new().unwrap();
    let ctx = Context::new(core.remote());
    let hdl = core.handle();
    let addr = "127.0.0.1:1234".parse().unwrap();
    let sock = UdpSocket::bind(&addr, &hdl).unwrap();
    let tcp = TcpListener::bind(&addr, &hdl).unwrap();
    let conn = Connection::new(sock, tcp, core.handle(), ctx);
    info!("[+] SQA Backend is up & running!");
    core.run(conn).unwrap();
}
