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
mod actions;
mod connection;

use sync::{UIContext, BackendContext};
fn main() {
    println!("SQA UI, using version {}", sqa_backend::VERSION);
    println!("an eta project <http://theta.eu.org>");
    println!("[+] Initialising GTK+");
    let _ = gtk::init().unwrap();
    println!("[+] Initialising event loop & backend context");
    let tn = util::ThreadNotifier::new();
    let ttn = tn.clone();
    let (btx, brx) = mpsc::unbounded();
    let (utx, urx) = smpsc::channel();
    let tutx = utx.clone();
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
    println!("[+] Initialising UI context");
    let b = Builder::new_from_string(util::INTERFACE_SRC);
    let mut ctx = UIContext {
        rx: urx,
        tx: btx,
        stn: tn.clone(),
        stx: utx,
        conn: connection::ConnectionController::new(&b),
        act: actions::ActionController::new(&b)
    };
    ctx.bind_all();
    let ctx = RefCell::new(ctx);
    tn.register_handler(move || {
        ctx.borrow_mut().on_event();
    });
    println!("[+] Showing main window");
    let win: Window = b.get_object("sqa-main").unwrap();
    win.show_all();
    println!("[+] Starting GTK+ event loop!");
    gtk::main();
}
