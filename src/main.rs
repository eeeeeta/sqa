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
mod parser;
mod command;
mod state;

use mixer::{Source, Sink};
use portaudio as pa;
use std::error::Error;

use gtk::prelude::*;
use gtk::{Builder, Entry, Label, Window, ListBox, Arrow, Widget};
use gtk::Box as GBox;
use gdk::enums::key as gkey;
use std::sync::{Arc, Mutex};
use std::thread;
use state::{ReadableContext, WritableContext, ObjectType};
use std::sync::mpsc::{Sender, Receiver, channel};
use command::{Command, Hunk, HunkTypes};
use std::rc::Rc;
use std::cell::RefCell;

fn build_hunk(hunk: &Box<Hunk>) -> Label {
    let st: String;
    println!("{:?}", hunk.disp());
    match hunk.disp() {
        HunkTypes::FilePath => {
            let val = hunk.get_val();
            if let Some(v) = val {
                st = format!("FilePath [{}]", v.downcast::<String>().unwrap())
            }
            else {
                st = format!("empty FilePath");
            }
        },
        HunkTypes::String => {
            let val = hunk.get_val();
            if let Some(v) = val {
                st = format!("String [{}]", v.downcast::<String>().unwrap())
            }
            else {
                st = format!("empty String");
            }
        },
        HunkTypes::Label => {
            let val = hunk.get_val();
            if let Some(v) = val {
                st = format!("Label [{}]", v.downcast::<String>().unwrap())
            }
            else {
                st = format!("empty Label");
            }
        },

        _ => unimplemented!()
    }
    Label::new(Some(&st))
}
fn main() {
    let _ = gtk::init().unwrap();
    let ui_src = include_str!("interface.glade");
    let builder = Builder::new_from_string(ui_src);
    let win: Window = builder.get_object("SQA Main Window").unwrap();
    let line: GBox = builder.get_object("CommandLine").unwrap();
    let ctx = ReadableContext::new();
    let lcmd: Rc<RefCell<Box<Command>>> = Rc::new(RefCell::new(Box::new(command::LoadCommand::new())));
    let mut hunks = lcmd.borrow().get_hunks();
    for hnk in &mut hunks {
        hnk.assoc(lcmd.clone());
        if let HunkTypes::FilePath = hnk.disp() {
            println!("{:?}", hnk.set_val(&ctx, Some(Box::new(String::from("blarg.aiff")))));
        }
        line.pack_start(&build_hunk(hnk), true, true, 0);
    }
    win.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });
    win.show_all();
    gtk::main();
}
