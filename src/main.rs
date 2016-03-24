extern crate rsndfile;
extern crate portaudio;
use rsndfile::SndFile;
use std::thread;
use std::time::Duration;
mod stream;
fn main() {
    let sfi = SndFile::open("test.aiff");
    println!("{:?}", sfi);
    println!("Go go gadget PortAudio!");
    let mut pa = portaudio::PortAudio::new().unwrap();
    let mut strm = stream::from_file(&mut pa, sfi.unwrap()).unwrap();
    thread::sleep(Duration::from_millis(2000));
    strm.run();
}

