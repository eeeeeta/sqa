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
use state::{ReadableContext, ThreadNotifier};
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
    let ctx = Arc::new(Mutex::new(ReadableContext::new()));
    let cc = ctx.clone();
    let (stx, srx) = channel();
    let tn = ThreadNotifier::new();
    let ttn = tn.clone();
    thread::spawn(move || {
        backend::backend_main(cc, stx, ttn);
        panic!("backend died :(");
    });
    let tncc = ctx.clone();
    let statebox: ListBox = builder.get_object("active-command-list").unwrap();
    tn.register_handler(move || {
        for chld in statebox.get_children() {
            chld.destroy();
        }
        for act in tncc.lock().unwrap().acts.iter() {
            let lbl = Label::new(None);
            let hours = act.runtime.num_hours();
            let minutes = act.runtime.num_minutes() - (60 * act.runtime.num_hours());
            let seconds = act.runtime.num_seconds() - (60 * act.runtime.num_minutes());
            lbl.set_markup(&format!("<b>{:02}:{:02}:{:02}</b> {:?}: {}", hours, minutes, seconds, act.state, act.desc));
            lbl.show_all();
            statebox.add(&lbl);
        }
    });
    println!("[+] Waiting for backend...");
    let sender = srx.recv().unwrap();
    println!("[+] Setting up window & GTK objects...");
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
    println!("[+] Initialisation complete!");
    gtk::main();
}
