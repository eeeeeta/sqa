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
mod command;
mod commands;
mod state;

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

fn build_hunk(hunk: Rc<RefCell<Box<Hunk>>>, hunks: Rc<RefCell<Vec<Rc<RefCell<Box<Hunk>>>>>>) -> EventBox {
    let st: String;
    {
     let hnk = hunk.borrow();
        println!("{:?}", hnk.disp());
     match hnk.disp() {
         HunkTypes::FilePath => {
             let val = hnk.get_val();
             if let Some(v) = val {
                 st = format!("FilePath [{}]", v.downcast::<String>().unwrap())
             }
             else {
                 st = format!("empty FilePath");
             }
         },
         HunkTypes::String => {
             let val = hnk.get_val();
             if let Some(v) = val {
                 st = format!("String [{}]", v.downcast::<String>().unwrap())
             }
             else {
                 st = format!("empty String");
             }
         },
         HunkTypes::Label => {
             let val = hnk.get_val();
             if let Some(v) = val {
                 st = format!("Label [{}]", v.downcast::<String>().unwrap())
             }
             else {
                 st = format!("empty Label");
             }
         },

         _ => unimplemented!()
     }
    }
    let eb = EventBox::new();
    let label = Label::new(Some(&st));
    label.set_margin_end(10);
    label.set_margin_start(10);
    let ui_src = include_str!("interface.glade");
    let builder = Builder::new_from_string(ui_src);
    let popover: Popover = builder.get_object("Edity").unwrap();
    let entry: Entry = builder.get_object("EdityEntry").unwrap();
    popover.set_relative_to(Some(&label));
    popover.set_position(::gtk::PositionType::Bottom);
    eb.connect_button_press_event(move |_, ev| {
        if let EventType::ButtonPress = ev.get_event_type() {
        println!("Go, edity!");
            popover.show_all();
        }
        Inhibit(false)
    });
    eb.add(&label);
    entry.connect_activate(move |lbl| {
        println!("{:?}", hunk.borrow_mut().set_val(&ReadableContext::new(), Some(Box::new(lbl.get_text().unwrap()))));
        let hnk = hunk.borrow();
        let st: String;
        println!("{:?}", hnk.disp());
        match hnk.disp() {
            HunkTypes::FilePath => {
                let val = hnk.get_val();
                if let Some(v) = val {
                    st = format!("FilePath [{}]", v.downcast::<String>().unwrap())
                }
                else {
                    st = format!("empty FilePath");
                }
            },
            HunkTypes::String => {
                let val = hnk.get_val();
                if let Some(v) = val {
                    st = format!("String [{}]", v.downcast::<String>().unwrap())
                }
                else {
                    st = format!("empty String");
                }
            },
            HunkTypes::Label => {
                let val = hnk.get_val();
                if let Some(v) = val {
                    st = format!("Label [{}]", v.downcast::<String>().unwrap())
                }
                else {
                    st = format!("empty Label");
                }
            },

            _ => unimplemented!()
        }
        label.set_text(&st);
    });
    eb
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
    let mut nhnks = Rc::new(RefCell::new(Vec::new()));
    {
    let mut nh = nhnks.borrow_mut();
    for mut hnk in hunks.into_iter() {
        hnk.assoc(lcmd.clone());
        nh.push(Rc::new(RefCell::new(hnk)));
    }
    for hnk in nh.iter() {
        line.pack_start(&build_hunk(hnk.clone(), nhnks.clone()), false, false, 0);
    }
        }
    win.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });
    win.show_all();
    gtk::main();
}
