#![feature(step_by)]

extern crate hound;
extern crate sqa_engine;
use std::thread;
use std::io::{self, Read};
use sqa_engine::{EngineContext, jack, Sender};
fn main() {
    let mut ec = EngineContext::new(None).unwrap();
    let mut reader = hound::WavReader::open("test.wav").unwrap();
    let mut chans = vec![];
    let mut ctls = vec![];
    for ch in 0..reader.spec().channels*16 {
        let st = format!("channel {}", ch);
        let p = ec.new_channel(&st).unwrap();
        let mut send = ec.new_sender(reader.spec().sample_rate as u64);
        send.set_output_patch(p);
        ctls.push(send.make_plain());
        chans.push((p, send));
    }
    for (i, port) in ec.conn.get_ports(None, None, Some(jack::PORT_IS_INPUT | jack::PORT_IS_PHYSICAL)).unwrap().into_iter().enumerate() {
        if i % 2 == 0 {
            for ch in (0..chans.len()).step_by(2) {
                ec.conn.connect_ports(&ec.chans[chans[ch].0], &port).unwrap();
            }
        }
        else {
            for ch in (1..chans.len()).step_by(2) {
                ec.conn.connect_ports(&ec.chans[chans[ch].0], &port).unwrap();
            }
        }

    }
    let _ = thread::spawn(move || {
        let mut idx = 0;
        let mut cnt = 0;
        for samp in reader.samples::<f32>() {
            let samp = samp.unwrap();
            for ch in (idx..chans.len()).step_by(2) {
                chans[ch].1.buf.push(samp * 0.1);
            }
            idx += 1;
            cnt += 1;
            if cnt == 500_000 {
                println!("Haha, random buffering fail for 5 seconds!!!");
                ::std::thread::sleep(::std::time::Duration::new(5, 0));
                println!("Alright, panic over.");
            }
            if idx >= 2 {
                idx = 0;
            }
        }
    });
    println!("*** Press Enter to begin playback!");
    io::stdin().read(&mut [0u8]).unwrap();
    let time = Sender::<()>::precise_time_ns();
    for ch in ctls.iter_mut() {
        ch.set_start_time(time);
        ch.set_active(true);
    }
    let mut secs = 0;
    loop {
        thread::sleep(::std::time::Duration::new(1, 0));
        secs += 1;
        println!("{}: {} samples - vol {}", ctls[0].position(), ctls[0].position_samples(), ctls[0].volume());
        if secs == 20 {
            println!("Haha, some sadist set ch0's active to false for 5 seconds!!!");
            ctls[0].set_active(false);
        }
        if secs == 25 {
            ctls[0].set_active(true);
            println!("Alright, panic over.");
        }
        if secs > 25 && secs < 36 {
            ctls[0].set_volume((secs - 25) as f32 * 0.1);
        }
        if secs > 60 {
            break;
        }
    }
}
