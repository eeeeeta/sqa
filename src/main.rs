extern crate rsndfile;
extern crate portaudio;
extern crate chrono;
use rsndfile::SndFile;
use std::thread;
use std::time::Duration;
mod streamv2;
mod mixer;
use mixer::*;
use portaudio as pa;
fn main() {
    let mut pa = pa::PortAudio::new().unwrap();
    let mut mixer = mixer::RudimentaryMixer::new(&mut pa, 500).unwrap();
    let file = SndFile::open("test.aiff").unwrap();
    let file2 = SndFile::open("meows.aiff").unwrap();
    println!("Loading file...");
    let streams = streamv2::FileStream::new(file);
    let mut xctl = streams[0].get_x();
    let mut rxctl = streams[1].get_x();
    let streams2 = streamv2::FileStream::new(file2);
    let mut chan_l = mixer::QChannel::new(44_100);
    let mut chan_r = mixer::QChannel::new(44_100);
    for (i, ch) in streams.into_iter().enumerate() {
        if i == 0 {
            chan_l.add_client(Box::new(ch));
        }
        else {
            chan_r.add_client(Box::new(ch));
        }
    }
    for (i, ch) in streams2.into_iter().enumerate() {
        if i == 0 {
            chan_l.add_client(Box::new(ch));
        }
        else {
            chan_r.add_client(Box::new(ch));
        }
    }

    println!("Here goes nothing...");
    chan_l.frames_hint(500);
    chan_r.frames_hint(500);
    *mixer.c1.lock().unwrap() = Some(Box::new(chan_l));
    *mixer.c2.lock().unwrap() = Some(Box::new(chan_r));
    mixer.stream.start().unwrap();
    thread::sleep(Duration::from_millis(5000));
    println!("Testing reset_pos()...");
    xctl.reset_pos(2_400_000);
    thread::sleep(Duration::from_millis(3000));
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
    while let true = mixer.stream.is_active().unwrap() {};
    mixer.stream.stop().unwrap();
}
