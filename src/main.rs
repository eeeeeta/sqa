#![feature(borrow_state, question_mark, iter_arith, dotdot_in_tuple_patterns)]
extern crate rsndfile;
extern crate portaudio;
extern crate chrono;
extern crate time;
extern crate uuid;
extern crate crossbeam;
extern crate rustbox;
extern crate gtk;
extern crate gdk;
extern crate bounded_spsc_queue;
extern crate mio;
extern crate glib;
#[macro_use]
extern crate mopa;
extern crate threadpool;
mod streamv2;
mod mixer;
#[macro_use]
mod command;
mod commands;
mod state;
mod ui;
mod backend;
mod cues;

use gtk::prelude::*;
use gtk::{Builder, Window};
use std::thread;
use state::{ThreadNotifier, Message};
use std::sync::mpsc::{channel};
use ui::UIContext;


fn main() {
    println!("SQA alpha 2, an eta thing");
    println!("[+] Initialising GTK & CSS contexts...");
    let _ = gtk::init().unwrap();
    let ui_src = ui::INTERFACE_SRC;
    let builder = Builder::new_from_string(ui_src);

    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("interface.css")).unwrap();
    let screen = gdk::Screen::get_default().unwrap();
    gtk::StyleContext::add_provider_for_screen(&screen, &provider, 0);
    println!("[+] Initialising backend...");
    let (stx, srx) = channel();
    let (tx, rx) = channel();
    let uitx = tx.clone();
    let tn = ThreadNotifier::new();
    let ttn = tn.clone();
    thread::spawn(move || {
        backend::backend_main(stx, tx, ttn);
        panic!("backend died :(");
    });
    println!("[+] Waiting for backend...");
    let sender = srx.recv().unwrap();
    println!("[+] Setting up window & GTK objects...");
    let win: Window = builder.get_object("SQA Main Window").unwrap();
    let uic = UIContext::init(sender, uitx, rx, tn, win, &builder);
    println!("[+] Initialisation complete!");
    gtk::main();
}

