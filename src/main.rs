extern crate rsndfile;
extern crate portaudio;
extern crate chrono;
extern crate uuid;
use rsndfile::SndFile;
use std::thread;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use std::time::Duration;
mod streamv2;
mod mixer;
use mixer::{Source,Sink};
use portaudio as pa;

fn main() {
    let mut pa = pa::PortAudio::new().unwrap();
    let mut mstr = mixer::Magister::new();

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
    let file = SndFile::open("test.aiff").unwrap();
    let file2 = SndFile::open("meows.aiff").unwrap();
    println!("[+] Loading file...");
    let streams = streamv2::FileStream::new(file);
    let mut xctl = streams[0].get_x();
    let mut rxctl = streams[1].get_x();
    let streams2 = streamv2::FileStream::new(file2);

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

    println!("L channel UUID: {}", c1_u);
    println!("R channel UUID: {}", c2_u);
    for (i, ch) in streams.into_iter().enumerate() {
        let uuid = ch.uuid();
        println!("Stream 1-{}: source UUID: {}", i, uuid);
        mstr.add_source(Box::new(ch));
            println!("Wiring to L channel: {:?}", mstr.wire(uuid, c1_u));
    }
    for (i, ch) in streams2.into_iter().enumerate() {
        let uuid = ch.uuid();
        println!("Stream 2-{}: source UUID: {}", i, uuid);
        mstr.add_source(Box::new(ch));
            println!("Wiring to R channel: {:?}", mstr.wire(uuid, c2_u));
    }
    chan_l.frames_hint(500);
    chan_r.frames_hint(500);
    mstr.add_source(Box::new(chan_l));
    mstr.add_source(Box::new(chan_r));
    for (i, out) in out_uuids.iter().enumerate() {
        println!("Output chan {} UUID: {}", i, out);
        match i {
            0 => println!("Wiring L channel: {:?}", mstr.wire(c1_up, *out)),
            1 => println!("Wiring R channel: {:?}", mstr.wire(c2_up, *out)),
            _ => println!("unknown :(")
        }
    }
    println!("Here goes nothing...");
    thread::sleep(Duration::from_millis(5000));
    println!("Testing reset_pos()...");
    xctl.reset_pos(2_400_000);
    thread::sleep(Duration::from_millis(7000));
    println!("Testing pause()...");
    xctl.pause();
    rxctl.pause();
    thread::sleep(Duration::from_millis(3000));
    println!("Testing unpause()...");
    xctl.unpause();
    rxctl.unpause();
    thread::sleep(Duration::from_millis(3000));
    println!("Testing set_vol()...");
    xctl.set_vol(0.5);
    rxctl.set_vol(0.5);
    thread::sleep(Duration::from_millis(3000));
    println!("Testing wiring...");
    println!("Rewiring Q1 -> R: {:?}", mstr.wire(c1_up, out_uuids[1]));
    println!("Rewiring Q2 -> L: {:?}", mstr.wire(c2_up, out_uuids[0]));
    loop {};
}
