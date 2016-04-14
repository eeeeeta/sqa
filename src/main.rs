extern crate rsndfile;
extern crate portaudio;
extern crate time;
extern crate uuid;
extern crate crossbeam;

mod streamv2;
mod mixer;
mod cues;

use std::thread;
use uuid::Uuid;
use time::Duration;
use std::rc::Rc;
use std::cell::RefCell;
use mixer::{Source, Sink, FRAMES_PER_CALLBACK};
use cues::{Q, AudioQ, FadeQ, QParam, WireableInfo, QList};
use portaudio as pa;
fn inspect_q_params(q: &Q) {
    for (strn, uuid, qp) in q.get_params() {
        println!("[*] \"{}\" (UUID {}): {:?}", strn, uuid, qp);
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
    let uu1 = aq1.uuid();
    println!("[+] AQ1 parameter listing:");
    inspect_q_params(&aq1);
    let mut aq2 = AudioQ::new(wrapped_mstr.clone());
    let uu2 = aq2.uuid();
    println!("[+] AQ2 parameter listing:");
    inspect_q_params(&aq1);
    println!("[+] Setting up QList...");
    let mut ql = QList::new();
    println!("[+] Setting file paths...");
    let fpu1 = aq1.get_params()[0].1;
    let fpu2 = aq2.get_params()[0].1;
    println!("[*] Setting AQ1's path as './test.aiff': {:?}", aq1.set_param(fpu1, QParam::FilePath(Some(format!("./test.aiff")))));
    println!("[*] Setting AQ2's path as './meows.aiff': {:?}", aq2.set_param(fpu2, QParam::FilePath(Some(format!("./meows.aiff")))));
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
    ql.insert(Box::new(aq1));
    ql.insert(Box::new(aq2));

    println!("\n[+] Creating Fade Cue...");
    let mut fq1 = FadeQ::new();
    println!("[*] FQ params:");
    inspect_q_params(&fq1);
    let fpu25 = fq1.get_params()[3].1;
    println!("[+] Setting fade time to 10s... {:?}", fq1.set_param(fpu25, QParam::Duration(Duration::milliseconds(10000))));
    println!("[*] FQ warnings: {:?}", fq1.warnings(&ql));
    println!("[+] Targeting chans of AQ1...");
    let fpu3 = fq1.get_params()[1].1;
    let fpu4 = fq1.get_params()[0].1;
    println!("[*] Targeting AQ1... {:?}", fq1.set_param(fpu3, QParam::UuidTarget(Some(uu1))));
    println!("[*] FQ warnings: {:?}", fq1.warnings(&ql));
    for ch in ql.cues.get(&uu1).unwrap().get_params() {
        if let QParam::Volume(chan, _) = ch.2 {
            println!("[*] Targeting channel {}... {:?}", chan, fq1.set_param(fpu4, QParam::VecInsert(0, Box::new(QParam::UuidTarget(Some(ch.1))))));
        }
    }
    println!("[*] FQ warnings: {:?}", fq1.warnings(&ql));
    println!("\n[+] Coolio. Hitting go on all cues...");
    ql.cues.get_mut(&uu1).unwrap().go();
    ql.cues.get_mut(&uu2).unwrap().go();
    fq1.go();
        thread::sleep(::std::time::Duration::from_millis(3000));
    println!("[+] Starting FQ loop...");
    loop {
        println!("polling...");
        thread::sleep(::std::time::Duration::from_millis(100));
        fq1.poll(&mut ql, Duration::milliseconds(100));
    }
}
