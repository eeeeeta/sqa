extern crate rsndfile;
extern crate portaudio;
extern crate chrono;
use rsndfile::SndFile;
use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};
mod stream;

fn main() {
    let sfi = SndFile::open("test.aiff");
    let meows = SndFile::open("meows.aiff");
    println!("Go go gadget PortAudio!");
    let mut strm = stream::Stream::new(sfi.unwrap()).unwrap();
    let mut meow_strm = stream::Stream::new(meows.unwrap()).unwrap();
    thread::sleep(Duration::from_millis(2000));
    strm.attach(Box::new(stream::FadeController::new((-20.0, 0.0), 100, 10000)));
    meow_strm.attach(Box::new(stream::FadeController::new((-20.0, 0.0), 100, 5000)));
    let h1 = strm.run();
    thread::sleep(Duration::from_millis(5000));
    println!("MEOWS ENGAGING");
    println!("{:?}", meow_strm.run().join());
    h1.join();
}

