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
use rustbox::{Key, RustBox, Color};
fn w(rb: &mut RustBox, x: usize, y: usize, text: &str) {
    rb.print(x, y, rustbox::RB_BOLD, Color::White, Color::Black, text);
}

fn main() {
    let mut rb = match RustBox::init(Default::default()) {
        Result::Ok(v) => v,
        Result::Err(e) => panic!("{}", e),
    };
    let mut parser = CmdParserFSM::new();
    let mut ctx = Context::new();
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
                    _ => {}
                }
            },
            Err(e) => panic!("{}", e.description()),
            _ => {}
        }
    }
    /*
    let mut pa = pa::PortAudio::new().unwrap();
    let mut mstr = mixer::Magister::new();
    let mut streams: BTreeMap<String, Vec<FileStreamX>> = BTreeMap::new();
    println!("[+] Initialising output device...");

    let def_out = pa.default_output_device().unwrap();
    let out_uuids: Vec<Uuid> = mixer::DeviceSink::from_device_chans(&mut pa, def_out)
        .unwrap()
        .into_iter()
        .map(|x| {
            let ret = x.uuid();
            mstr.add_sink(Box::new(x));
            ret
        })
        .collect();

    println!("[+] Initialising QChannels & mixing...");

    let mut chan_l = mixer::QChannel::new(44_100);
    let chan_l_x = chan_l.get_x();
    let c1_u = chan_l_x.uuid();
    let c1_up = chan_l_x.uuid_pair();
    mstr.add_sink(Box::new(chan_l_x));

    let mut chan_r = mixer::QChannel::new(44_100);
    let chan_r_x = chan_r.get_x();
    let c2_u = chan_r_x.uuid();
    let c2_up = chan_r_x.uuid_pair();
    mstr.add_sink(Box::new(chan_r_x));

    println!("[*] L channel UUID: {}", c1_u);
    println!("[*] R channel UUID: {}", c2_u);

    chan_l.frames_hint(FRAMES_PER_CALLBACK);
    chan_r.frames_hint(FRAMES_PER_CALLBACK);
    mstr.add_source(Box::new(chan_l));
    mstr.add_source(Box::new(chan_r));

    for (i, out) in out_uuids.iter().enumerate() {
        match i {
            0 => println!("[*] Wiring Q1 to output: {:?}", mstr.wire(c1_up, *out)),
            1 => println!("[*] Wiring Q2 to output: {:?}", mstr.wire(c2_up, *out)),
            _ => {}
        }
    }
    println!("\n[+] Right! Type away.");
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let ln = line.unwrap();
        let res = cmdi::command(&ln);
        if let ::nom::IResult::Done(left, cmd) = res {
            if left.len() > 0 {
                println!("[*] warning: unparsed section '{}'", left);
            }
            match cmd {
                Command::Load(filen, optident) => {
                    let file = SndFile::open(filen);
                    if let Err(snde) = file {
                        println!("[-] couldn't open file: {}", snde.expl);
                        continue;
                    }
                    if file.as_ref().unwrap().info.samplerate != 44_100 {
                        println!("[-] sample rate mismatch");
                        continue;
                    }
                    let fs = FileStream::new(file.unwrap());
                    let mut cvec = vec![];
                    for stream in fs.into_iter() {
                        cvec.push(stream.get_x());
                        mstr.add_source(Box::new(stream));
                    }
                    for ch in cvec.iter() {
                        mstr.wire(ch.uuid(), c1_u).unwrap();
                    }
                    let mut ident: &str = ::std::path::Path::new(filen).file_stem().unwrap().to_str().unwrap();
                    if optident.is_some() {
                        ident = optident.unwrap();
                    }
                    streams.insert(ident.to_owned(), cvec);
                    println!("[+] Loaded '{}' using identifier '{}'", filen, ident);
                },
                Command::Vol(ident, chan, vol, fade) => {
                    if let Some(fsx) = streams.get_mut(ident) {
                        if chan >= fsx.len() as i32 {
                            println!("[-] invalid channel");
                            continue;
                        }
                        for (i, ch) in fsx.iter_mut().enumerate() {
                            if chan == -1 || chan as usize == i {
                                println!("[*] Setting ${}:{} to {}dB ({})", ident, i, vol, db_lin(vol));
                                ch.set_vol(db_lin(vol));
                            }
                        }
                    }
                    else {
                        println!("[-] unknown identifier");
                    }
                },
                _ => unimplemented!()
            }
        }
        else {
            println!("[-] parse failed: {:?}", res);
        }
    }*/
}
