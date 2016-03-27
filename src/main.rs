extern crate rsndfile;
extern crate portaudio;
extern crate chrono;
use rsndfile::SndFile;
use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};
mod stream;
mod streamv2;
mod mixer;
use mixer::*;
use portaudio as pa;
fn main() {
    let mut pa = pa::PortAudio::new().unwrap();
    let file = SndFile::open("test.aiff").unwrap();
    let file2 = SndFile::open("meows.aiff").unwrap();
    println!("Loading file...");
    let streams = streamv2::FileStream::new(file);
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
    let mut mixer = mixer::RudimentaryMixer::new(&mut pa, Box::new(chan_l),
                                             Box::new(chan_r)).unwrap();
    mixer.stream.start().unwrap();
    while let true = mixer.stream.is_active().unwrap() {};
    mixer.stream.stop().unwrap();
}
