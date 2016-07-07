#![feature(borrow_state, question_mark)]
extern crate rsndfile;
extern crate portaudio;
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
mod streamv2;
mod mixer;
#[macro_use]
mod command;
mod commands;
mod state;
mod ui;
mod backend;

use gtk::prelude::*;
use gtk::{Builder, Window};
use gdk::enums::key as gkey;
use std::sync::{Arc, Mutex};
use std::thread;
use state::{ReadableContext, ThreadNotifier};
use std::sync::mpsc::{channel};
use ui::{CommandLine, CommandChooserController};

fn main() {
    let _ = gtk::init().unwrap();
    let ui_src = include_str!("interface.glade");
    let builder = Builder::new_from_string(ui_src);

    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("interface.css")).unwrap();
    let screen = gdk::Screen::get_default().unwrap();
    gtk::StyleContext::add_provider_for_screen(&screen, &provider, 0);

    let ctx = Arc::new(Mutex::new(ReadableContext::new()));
    let cc = ctx.clone();
    let (stx, srx) = channel();
    let tn = ThreadNotifier::new();
    let ttn = tn.clone();
    thread::spawn(move || {
        backend::backend_main(cc, stx, ttn);
        panic!("backend died :(");
    });
    tn.register_handler(move || {
        println!("yay, it worked");
    });
    let sender = srx.recv().unwrap();
    let win: Window = builder.get_object("SQA Main Window").unwrap();
    let cmdl = CommandLine::new(ctx.clone(), sender, &builder);
    let cc = CommandChooserController::new(cmdl.clone(), &builder);
    win.connect_key_press_event(move |_, ek| {
        if ek.get_state().contains(gdk::CONTROL_MASK) {
            match ek.get_keyval() {
                gkey::Return => {
                    CommandChooserController::toggle(cc.clone());
                    Inhibit(true)
                },
                _ => Inhibit(false)
            }
        }
        else {
            Inhibit(false)
        }
    });
    win.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });
    win.show_all();

    gtk::main();
}
