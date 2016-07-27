#![feature(borrow_state, question_mark)]
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
mod streamv2;
mod mixer;
#[macro_use]
mod command;
mod commands;
mod state;
mod ui;
mod backend;

use gtk::prelude::*;
use gtk::{Builder, Window, ListBox, Label};
use gdk::enums::key as gkey;
use std::sync::{Arc, Mutex};
use std::thread;
use state::{ThreadNotifier, Message};
use std::sync::mpsc::{channel};
use ui::{CommandLine, CommandChooserController};

fn main() {
    println!("SQA alpha 2, an eta thing");
    println!("[+] Initialising GTK & CSS contexts...");
    let _ = gtk::init().unwrap();
    let ui_src = include_str!("interface.glade");
    let builder = Builder::new_from_string(ui_src);

    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("interface.css")).unwrap();
    let screen = gdk::Screen::get_default().unwrap();
    gtk::StyleContext::add_provider_for_screen(&screen, &provider, 0);
    println!("[+] Initialising backend...");
    let (stx, srx) = channel();
    let (tx, rx) = channel();
    let tn = ThreadNotifier::new();
    let ttn = tn.clone();
    thread::spawn(move || {
        backend::backend_main(stx, tx, ttn);
        panic!("backend died :(");
    });
    let statebox: ListBox = builder.get_object("active-command-list").unwrap();
    println!("[+] Waiting for backend...");
    let sender = srx.recv().unwrap();
    println!("[+] Setting up window & GTK objects...");
    let win: Window = builder.get_object("SQA Main Window").unwrap();
    let cmdl = CommandLine::new(sender, &builder);
    let cc = CommandChooserController::new(cmdl.clone(), &builder);
    let cmdlc = cmdl.clone();
    let ccc = cc.clone();
    tn.register_handler(move || {
        match rx.recv().unwrap() {
            Message::CmdDesc(uu, desc) => {
                let which = {
                    let cl = cmdlc.borrow();
                    cl.uuid.is_some()
                };
                if which {
                    CommandLine::build(cmdlc.clone(), desc);
                }
                else {
                    CommandLine::update(cmdlc.clone(), Some(desc));
                }
                CommandChooserController::update(ccc.clone());
            },
            _ => unimplemented!()
        }
    });

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
    println!("[+] Initialisation complete!");
    gtk::main();
}

