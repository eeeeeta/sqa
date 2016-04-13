extern crate rsndfile;
extern crate portaudio;
extern crate chrono;
extern crate uuid;
extern crate crossbeam;

mod streamv2;
mod mixer;
mod cues;

use std::thread;
use uuid::Uuid;
use std::time::Duration;
use std::rc::Rc;
use std::cell::RefCell;
use mixer::{Source, Sink, FRAMES_PER_CALLBACK};
use cues::{Q, AudioQ, QParam, WireableInfo};
use portaudio as pa;

fn inspect_q_params(q: &Q) {
    for (uuid, qp) in q.get_params() {
        println!("[*] UUID {}: {:?}", uuid, qp);
    }
}

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
    let wrapped_mstr = Rc::new(RefCell::new(mstr));
    println!("[+] Creating some cool Audio Cues...");
    let mut aq1 = AudioQ::new(wrapped_mstr.clone());
    println!("[+] AQ1 parameter listing:");
    inspect_q_params(&aq1);
    let mut aq2 = AudioQ::new(wrapped_mstr.clone());
    println!("[+] AQ2 parameter listing:");
    inspect_q_params(&aq1);

    println!("[+] Setting file paths...");
    let fpu1 = aq1.get_params()[0].0;
    let fpu2 = aq2.get_params()[0].0;
    println!("[*] Setting AQ1's path as './test.aiff': {:?}", aq1.set_param(fpu1, QParam::FilePath(format!("./test.aiff"))));
    println!("[*] Setting AQ2's path as './meows.aiff': {:?}", aq2.set_param(fpu2, QParam::FilePath(format!("./meows.aiff"))));
    println!("[+] AQ1 parameter listing:");
    inspect_q_params(&aq1);
    println!("[+] AQ2 parameter listing:");
    inspect_q_params(&aq2);

    println!("[+] Wiring Audio Cues...");
    println!("[*] Getting Wireables of AQ1...");
    for WireableInfo(is_source, cid, uuid) in aq1.wireables() {
        println!("[*] Wireable: source {}, cid {}, uuid {}", is_source, cid, uuid);
        println!("[*] Wiring to Q1... {:?}", wrapped_mstr.borrow_mut().wire(uuid, c1_u));
    }
    println!("[*] Getting Wireables of AQ2...");
    for WireableInfo(is_source, cid, uuid) in aq2.wireables() {
        println!("[*] Wireable: source {}, cid {}, uuid {}", is_source, cid, uuid);
        println!("[*] Wiring to Q2... {:?}", wrapped_mstr.borrow_mut().wire(uuid, c2_u));
    }
    println!("\n[+] Here goes nothing! Hitting GO on AQ1...");
    aq1.go();
    thread::sleep(Duration::from_millis(5000));
    println!("[+] and on AQ2...");
    aq2.go();
    thread::sleep(Duration::from_millis(1000));
    println!("\n[+] Yay (hopefully)! Now for some tests...");
    thread::sleep(Duration::from_millis(5000));
    println!("[*] Resetting AQ1...");
    aq1.reset();
    thread::sleep(Duration::from_millis(1000));
    println!("[*] Hitting GO on AQ1...");
    aq1.go();
    thread::sleep(Duration::from_millis(7000));
    println!("[*] Pausing AQ2...");
    aq2.pause();
    thread::sleep(Duration::from_millis(3000));
    println!("[*] Hitting GO on AQ2...");
    aq2.go();
    thread::sleep(Duration::from_millis(3000));
    println!("[*] Screwing with the volume of AQ1...");
    for (uuid, qp) in aq1.get_params() {
        if let QParam::Volume(ch, vol) = qp {
            println!("[*] Setting ch{}'s vol to 0.5 (from {})... {:?}", ch, vol, aq1.set_param(uuid, QParam::Volume(ch, 0.5)));
        }
    }
    thread::sleep(Duration::from_millis(3000));
    println!("[*] Testing wiring...");
    println!("[*] Rewiring Q1 -> R: {:?}", wrapped_mstr.borrow_mut().wire(c1_up, out_uuids[1]));
    println!("[*] Rewiring Q2 -> L: {:?}", wrapped_mstr.borrow_mut().wire(c2_up, out_uuids[0]));
    println!("\n[+] Testing complete. Dying in 100 seconds...");
    thread::sleep(Duration::from_millis(100000));
}
