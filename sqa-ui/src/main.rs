extern crate gtk;
extern crate sqa_backend;
extern crate rosc;
extern crate tokio_core;
extern crate futures;
extern crate glib;
extern crate time;
#[macro_use] extern crate error_chain;
extern crate gdk;
extern crate uuid;
extern crate url;
#[macro_use] extern crate log;
extern crate fern;

use gtk::prelude::*;
use gtk::{Builder, Window};
use std::thread;
use tokio_core::reactor::Core;
use futures::sync::mpsc;
use std::sync::mpsc as smpsc;
use std::cell::RefCell;

#[macro_use]
mod util;
mod widgets;
mod errors;
mod sync;
mod messages;
mod actions;
mod connection;
mod save;

use sync::{UIContext, BackendContext};
fn main() {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!("[{}] {} {}", record.target(), record.level(), message))
        })
        .level(log::LogLevelFilter::Trace)
        .level_for("tokio_core", log::LogLevelFilter::Info)
        .level_for("mio", log::LogLevelFilter::Info)
        .chain(::std::io::stdout())
        .apply()
        .unwrap();

    info!("SQA UI, using version {}", sqa_backend::VERSION);
    info!("an eta project <http://theta.eu.org>");
    info!("[+] Initialising GTK+");
    let _ = gtk::init().unwrap();
    let b = Builder::new_from_string(util::INTERFACE_SRC);
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("ui.css")).unwrap();
    let screen = gdk::Screen::get_default().unwrap();
    gtk::StyleContext::add_provider_for_screen(&screen, &provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
    info!("[+] Initialising event loop & backend context");
    let tn = util::ThreadNotifier::new();
    let ttn = tn.clone();
    let (btx, brx) = mpsc::unbounded();
    let (utx, urx) = smpsc::channel();
    let tutx = utx.clone();
    let win: Window = b.get_object("sqa-main").unwrap();
    thread::spawn(move || {
        let mut core = Core::new().unwrap();
        let mut ctx = BackendContext {
            conn: connection::Context::new(),
            tn: ttn,
            rx: brx,
            tx: tutx,
            hdl: core.handle()
        };
        core.run(&mut ctx).unwrap();
        panic!("The future resolved! What is this sorcery?!");
    });
    info!("[+] Initialising UI context");
    let mut ctx = UIContext {
        rx: urx,
        tx: btx,
        stn: tn.clone(),
        stx: utx,
        conn: connection::ConnectionController::new(&b),
        act: actions::ActionController::new(&b),
        msg: messages::MessageController::new(&b),
        save: save::SaveController::new(&b, win.clone())
    };
    ctx.bind_all();
    let ctx = RefCell::new(ctx);
    tn.register_handler(move || {
        ctx.borrow_mut().on_event();
    });
    info!("[+] Showing main window");
    win.set_title(&format!("SQA UI [{}]", sqa_backend::VERSION));
    win.show_all();
    info!("[+] Starting GTK+ event loop!");
    gtk::main();
}
