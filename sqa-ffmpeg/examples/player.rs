extern crate sqa_engine;
extern crate sqa_ffmpeg;

use sqa_engine::{EngineContext, jack, Sender};
use sqa_engine::param::{Parameter, FadeDetails};
use sqa_ffmpeg::{MediaFile, init, Duration};
use std::io::{self, BufRead};
use std::time::Duration as StdDuration;
use std::thread;

fn main() {
    let mut mctx = init().unwrap();
    mctx.network_init().unwrap();
    println!("Provide a FFmpeg URL:");

    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let mut buffer = String::new();
    stdin.read_line(&mut buffer).unwrap();
    let mut file = MediaFile::new(&mut mctx, &buffer.trim()).unwrap();
    let mut ec = EngineContext::new(None).unwrap();
    let mut chans = vec![];
    let mut ctls = vec![];
    for ch in 0..file.channels() {
        let st = format!("channel {}", ch);
        let p = ec.new_channel(&st).unwrap();
        let mut send = ec.new_sender(file.sample_rate() as u64);
        send.set_output_patch(p);
        ctls.push(send.make_plain());
        chans.push((p, send));
    }
    for (i, port) in ec.conn.get_ports(None, None, Some(jack::PORT_IS_INPUT | jack::PORT_IS_PHYSICAL)).unwrap().into_iter().enumerate() {
        if let Some(ch) = ec.chans.get(i) {
            ec.conn.connect_ports(ch.as_ref().unwrap(), &port).unwrap();
        }
    }
    println!("Chans: {} Sample rate: {} Duration: {} Bitrate: {}", file.channels(), file.sample_rate(), file.duration(), file.bitrate());
    let _thr = ::std::thread::spawn(move || {
        loop {
            for x in &mut file {
                if let Ok(mut x) = x {
                    for (ch, smpl) in &mut x {
                        chans[ch].1.buf.push(smpl.f32());
                    }
                }
            }
            file.seek(Duration::seconds(0)).unwrap();
        }
    });
    let time = Sender::<()>::precise_time_ns();
    for ch in ctls.iter_mut() {
        ch.set_start_time(time);
        ch.set_active(true);
    }
    let mut secs = 0;
    loop {
        thread::sleep(::std::time::Duration::new(1, 0));
        secs += 1;
        println!("{}: {} samples - vol {:?}", ctls[0].position(), ctls[0].position_samples(), ctls[0].volume());
        if secs == 10 {
            println!("*** Linear fade test commencing!");
            let cur_vol = ctls[0].volume().get(0);
            let mut fd = FadeDetails::new(cur_vol, 0.0);
            fd.set_start_time(Sender::<()>::precise_time_ns() + sqa_engine::ONE_SECOND_IN_NANOSECONDS);
            fd.set_duration(StdDuration::new(10, 0));
            fd.set_active(true);
            for ch in ctls.iter_mut() {
                ch.set_volume(Box::new(Parameter::LinearFade(fd.clone())));
            }
            println!("*** The volume will fade to 0.0 over 10 seconds, commencing in 1 second.");
        }
        if secs == 25 {
            println!("*** Okay, returning to normal.");
            for ch in ctls.iter_mut() {
                ch.set_volume(Box::new(Parameter::Raw(1.0)));
            }
        }
    }
}
