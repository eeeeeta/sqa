extern crate rsndfile;
extern crate portaudio;
extern crate time;
extern crate uuid;
extern crate crossbeam;
extern crate rustbox;
extern crate gtk;
extern crate gdk;

mod streamv2;
mod mixer;
mod parser;
mod command;
mod commands;
mod state;
mod backend;

use mixer::{Source, Sink};
use portaudio as pa;
use command::CmdParserFSM;
use std::error::Error;

use gtk::prelude::*;
use gtk::{Builder, Entry, Label, Window, ListBox, Arrow};
use gdk::enums::key as gkey;
use std::sync::{Arc, Mutex};
use std::thread;
use state::{ReadableContext, WritableContext, ObjectType};
use std::sync::mpsc::{Sender, Receiver, channel};
use command::Command;
use std::rc::Rc;
use std::cell::RefCell;

fn update_arrow(err: bool, p: &mut CmdParserFSM, ctx: &ReadableContext, a: &Arrow) {
    if err {
        a.set(gtk::ArrowType::Up, gtk::ShadowType::None);
    }
    else {
        if p.would_enter(&ctx) {
            a.set(gtk::ArrowType::Down, gtk::ShadowType::None);
        }
        else {
            a.set(gtk::ArrowType::Right, gtk::ShadowType::None);
        }
    }
}
fn update_list(ctx: &ReadableContext, list: &ListBox) {
    for child in list.get_children() {
        list.remove(&child);
    }
    for (k, v) in ctx.db.iter() {
        if let ObjectType::FileStream(_, _) = v.typ {
        let mut row = gtk::ListBoxRow::new();
        let mut label = Label::new(Some(&format!("{} ({})", v, k)));
        println!("{} ({})", v, k);
        row.add(&label);
            list.insert(&row, -1);
        }

    }
    list.show_all();
}
fn main() {
    let _ = gtk::init().unwrap();
    let ui_src = include_str!("interface.glade");
    let builder = Builder::new_from_string(ui_src);
    let parser = Rc::new(RefCell::new(CmdParserFSM::new()));
    let rc = Arc::new(Mutex::new(ReadableContext::new()));
    let trc = rc.clone();
    let (tx, rx): (Sender<Box<Command>>, Receiver<Box<Command>>) = channel();
    thread::spawn(move || {
        backend::backend_main(trc, rx);
    });

    let win: Window = builder.get_object("MainWindow").unwrap();
    let cmdbox: Entry = builder.get_object("CmdBox").unwrap();
    let cmdmsg: Label = builder.get_object("CmdMessage").unwrap();
    let arrow: Arrow = builder.get_object("CmdArrow").unwrap();
    let olist: ListBox = builder.get_object("ObjectList").unwrap();
    win.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });
    cmdbox.connect_event(move |cbox, ev| {
        if let gdk::EventType::KeyPress = ev.get_event_type() {
            let k = ev.clone().downcast::<gdk::EventKey>().unwrap();
            let keyval = k.get_keyval();
            let mut p = parser.borrow_mut();
            let ctx = rc.lock().unwrap();
            if let gkey::BackSpace = keyval {
                cmdmsg.set_text(&format!("SQA is ready."));
                p.back();
                update_arrow(false, &mut p, &ctx, &arrow);
                cbox.set_text(&p.cmdline());
            }
            else if let gkey::Return = keyval {
                match p.enter(&ctx) {
                    Ok(cmd) => {
                        cmdmsg.set_text(&format!("SQA is ready."));
                        if let Some(cmd) = cmd {
                            tx.send(cmd).unwrap();
                            cmdmsg.set_text(&format!("Executed."));
                        }
                        cbox.set_text(&p.cmdline());
                        update_arrow(false, &mut p, &ctx, &arrow);
                    },
                    Err(e) => {
                        cmdmsg.set_markup(&format!("<span color=\"red\" weight=\"bold\">{}</span>", Into::<String>::into(e)));
                        update_arrow(true, &mut p, &ctx, &arrow);
                    }
                }
            }
            else if let Some(ch) = gdk::keyval_to_unicode(keyval) {
                match p.addc(ch, &ctx) {
                    Ok(()) => {
                        cmdmsg.set_text(&format!("SQA is ready."));
                        cbox.set_text(&p.cmdline());
                        update_arrow(false, &mut p, &ctx, &arrow);
                    },
                    Err(e) => {
                        cmdmsg.set_markup(&format!("<span color=\"red\" weight=\"bold\">{}</span>", Into::<String>::into(e)));
                        update_arrow(true, &mut p, &ctx, &arrow);
                    }
                }
            }
            cbox.set_position(-1);
            update_list(&ctx, &olist);
            Inhibit(true)
        }
        else {
            Inhibit(false)
        }
    });

    win.show_all();
    gtk::main();
}
/*
use mixer::{Source, Sink};
use portaudio as pa;
use command::CmdParserFSM;
use state::Context;
use std::error::Error;
use streamv2::lin_db;
use rustbox::{Key, RustBox, Color, InitOptions, InputMode};

fn w(rb: &mut RustBox, x: usize, y: usize, text: &str) {
    rb.print(x, y, rustbox::RB_NORMAL, Color::White, Color::Default, text);
}
fn w_emp(rb: &mut RustBox, x: usize, y: usize, text: &str) {
    rb.print(x, y, rustbox::RB_BOLD, Color::Yellow, Color::Default, text);
}

struct Counter(usize);
impl Counter {
    fn incr(&mut self) -> usize {
        let x = self.0;
        self.0 += 1;
        x
    }
}
fn main() {
    let mut rb = match RustBox::init(InitOptions {
            input_mode: InputMode::Current,
            buffer_stderr: true,
        }) {
        Result::Ok(v) => v,
        Result::Err(e) => panic!("{}", e),
    };
    let mut parser = CmdParserFSM::new();
    let mut pa = pa::PortAudio::new().unwrap();
    let mut ctx = Context::new();
    let idx = pa.default_output_device().unwrap();
    for (i, ch) in mixer::DeviceSink::from_device_chans(&mut pa, idx).unwrap().into_iter().enumerate() {
        let uu = ch.uuid();
        ctx.outs.push(uu.clone());
        ctx.mstr.add_sink(Box::new(ch));
        if i < 16 {
            ctx.mstr.wire(ctx.qchan_outs[i], uu).unwrap();
        }
    }
    let mut cmdline = String::new();
    let mut error = "Ready.".to_owned();
    loop {
        rb.clear();
        let mut ln = Counter(0);
        let half: isize = (rb.width() / 2) as isize - 22;
        if half > 2 {
            let half = half as usize;
            w(&mut rb, half, ln.incr(), &format!(" ____   ___      _    "));
            w(&mut rb, half, ln.incr(), &format!("/ ___| / _ \\    / \\   "));
            w(&mut rb, half, ln.incr(), &format!("\\___ \\| | | |  / _ \\  "));
            w(&mut rb, half, ln.incr(), &format!(" ___) | |_| | / ___ \\ "));
            w(&mut rb, half, ln.incr(), &format!("|____/ \\__\\_\\/_/   \\_\\"));
            w(&mut rb, half, ln.incr(), &format!("                      "));
            w(&mut rb, half-1, ln.incr(), &format!("alpha 1 - an eta project"));
        }
        w(&mut rb, 0, ln.incr(), &format!("{}", parser.debug_remove_me(&ctx)));
        w_emp(&mut rb, 0, ln.incr(), &format!("{}", error));
        w(&mut rb, 0, ln.incr(), &format!("> {}", cmdline));
        ln.incr();
        w(&mut rb, 0, ln.incr(), &format!("Loaded audio files:"));
        w(&mut rb, 0, ln.incr(), &format!("------------------"));
        for (k, v) in ctx.idents.iter() {
            for (i, ch) in v.iter().enumerate() {
                let lp = ch.lp();
                w(&mut rb, 0, ln.incr(), &format!("${}:{} - {:.2}dB, {:.1}%", k, i, lin_db(lp.vol), 100 as f64 * (lp.pos as f64 / lp.end as f64)));
            }
        }
        rb.present();
        match rb.peek_event(::std::time::Duration::from_millis(500), false) {
            Ok(rustbox::Event::KeyEvent(key)) => {
                match key {
                    Key::Ctrl('c') => { break; }
                    Key::Char(ch) => {
                        match parser.addc(ch, &ctx) {
                            Ok(p) => {
                                parser = p;
                                cmdline = parser.cmdline();
                            },
                            Err((p, e)) => {
                                parser = p;
                                error = format!("{}", Into::<String>::into(e));
                            }
                        }
                    },
                    Key::Backspace => {
                        parser = parser.back();
                        cmdline = parser.cmdline();
                    },
                    Key::Enter => {
                        match parser.enter(&mut ctx) {
                            Ok(p) => {
                                parser = p;
                                cmdline = String::new();
                                error = format!("Executed command.");
                            },
                            Err((p, e)) => {
                                parser = p;
                                cmdline = parser.cmdline();
                                error = e;
                            }
                        }
                    },
                    _ => {}
                }
            },
            _ => {}
        }
    }
}
*/
