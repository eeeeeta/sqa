extern crate rsndfile;
extern crate portaudio;
extern crate time;
extern crate uuid;
extern crate crossbeam;
extern crate rustbox;

mod streamv2;
mod mixer;
mod parser;
mod command;
mod state;

use std::thread;
use std::io;
use streamv2::db_lin;
use rsndfile::SndFile;
use std::io::BufRead;
use std::collections::BTreeMap;
use uuid::Uuid;
use time::Duration;
use std::rc::Rc;
use std::cell::RefCell;
use mixer::{Source, Sink, FRAMES_PER_CALLBACK};
use streamv2::{FileStream, FileStreamX};
use portaudio as pa;

use command::CmdParserFSM;
use state::Context;
use std::error::Error;
use rustbox::{Key, RustBox, Color, InitOptions, InputMode};

fn w(rb: &mut RustBox, x: usize, y: usize, text: &str) {
    rb.print(x, y, rustbox::RB_BOLD, Color::White, Color::Black, text);
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
    let mut error = String::new();
    loop {
        rb.clear();
        w(&mut rb, 0, 0, &format!("parser: {}", parser.debug_remove_me(&ctx)));
        w(&mut rb, 0, 1, &format!("last error: {}", error));
        w(&mut rb, 0, 2, &format!("> {}", cmdline));
        rb.present();
        match rb.poll_event(false) {
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
                                error = format!("{:?}", e);
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
            Err(e) => panic!("{}", e.description()),
            _ => {}
        }
    }
}
