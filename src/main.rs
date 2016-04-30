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

use mixer::{Source, Sink};
use portaudio as pa;
use command::CmdParserFSM;
use state::Context;
use std::error::Error;
use rustbox::{Key, RustBox, Color, InitOptions, InputMode};

fn w(rb: &mut RustBox, x: usize, y: usize, text: &str) {
    rb.print(x, y, rustbox::RB_NORMAL, Color::White, Color::Default, text);
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
        w(&mut rb, 0, ln.incr(), &format!("{}", error));
        w(&mut rb, 0, ln.incr(), &format!("> {}", cmdline));
        ln.incr();
        w(&mut rb, 0, ln.incr(), &format!("Loaded audio files:"));
        w(&mut rb, 0, ln.incr(), &format!("------------------"));
        for (k, v) in ctx.idents.iter() {
            for (i, ch) in v.iter().enumerate() {
                let lp = ch.lp();
                w(&mut rb, 0, ln.incr(), &format!("${}:{} - {}dB, {:.1}%", k, i, lp.vol, 100 as f64 * (lp.pos as f64 / lp.end as f64)));
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
            _ => {}
        }
    }
}
