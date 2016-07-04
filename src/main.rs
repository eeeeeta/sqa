#![feature(borrow_state)]
extern crate rsndfile;
extern crate portaudio;
extern crate time;
extern crate uuid;
extern crate crossbeam;
extern crate rustbox;
extern crate gtk;
extern crate gdk;
#[macro_use]
extern crate mopa;
mod streamv2;
mod mixer;
#[macro_use]
mod command;
mod commands;
mod state;
mod ui;

use mixer::{Source, Sink};
use portaudio as pa;
use std::error::Error;

use gtk::prelude::*;
use gtk::{Builder, Entry, Label, Window, ListBox, EventBox, Popover, Arrow, Widget};
use gdk::EventType;
use gtk::Box as GBox;
use gdk::enums::key as gkey;
use std::sync::{Arc, Mutex};
use std::thread;
use state::{ReadableContext, WritableContext, ObjectType};
use std::sync::mpsc::{Sender, Receiver, channel};
use command::{Command, Hunk, HunkTypes};
use std::rc::Rc;
use std::cell::RefCell;
use commands::*;
use ui::{CommandLine, CommandChooserController};

fn main() {
    let _ = gtk::init().unwrap();
    let ui_src = include_str!("interface.glade");
    let builder = Builder::new_from_string(ui_src);
    let win: Window = builder.get_object("SQA Main Window").unwrap();
    let ctx = Rc::new(RefCell::new(ReadableContext::new()));
    let cmdl = CommandLine::new(ctx.clone(), &builder);
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
