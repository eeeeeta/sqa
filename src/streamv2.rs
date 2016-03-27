
use rsndfile::{SndFile, SndFileInfo};
use std::time::Duration;
use std::sync::{Arc, RwLock, Mutex};
use std::ops::DerefMut;
use mixer;
/// Converts a linear amplitude to decibels.
fn lin_db(lin: f32) -> f32 {
    lin.log10() * 20.0
}
/// Converts a decibel value to a linear amplitude.
fn db_lin(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}
/// Contains information about the stream currently playing.
///
/// This is copied about the place and modified by the `StreamController`
/// when it makes its changes. It is then copied into the stream callback
/// and its state made consistent with the stream thread's internal state.
#[derive(Debug, Clone, Copy)]
pub struct LiveParameters {
    /// Current volume (linear amplitude)
    pub vol: f32,
    /// Amount of frames written so far.
    pos: usize
}
/// Something that can control a playing stream.
pub trait StreamController {
    /// Called to determine how long to sleep before the controller's `ctl` function
    /// is called again.
    fn accuracy(&self) -> Duration;
    /// Modifies a given `LiveParameters` with whatever the controller wishes to do.
    ///
    /// Returns `None` if the controller has finished controlling the stream, and
    /// wishes to relinquish its control.
    fn ctl(&mut self, last: LiveParameters) -> Option<LiveParameters>;
    fn init(&mut self, last: LiveParameters) -> LiveParameters;
}

pub struct FileStream<'a> {
    file: Arc<Mutex<SndFile>>,
    refill_buf: Arc<Mutex<Vec<f32>>>,
    buf: Arc<RwLock<Vec<Vec<f32>>>>,
    refilling: Arc<Mutex<()>>,
    fill_len: Arc<RwLock<usize>>,
    sample_rate: u64,
    id: usize,
    lp: LiveParameters,
    controller: Option<Box<StreamController + 'a>>
}

impl<'a> FileStream<'a> {
    pub fn refill(&mut self) {
        let write_lck = self.refilling.try_lock();
        let mut vecs: ::std::sync::RwLockWriteGuard<Vec<Vec<f32>>>;
        if let Ok(_) = write_lck {
            println!("Refill lock attained!");
            vecs = self.buf.write().unwrap();
        }
        else {
            println!("Refill in progress");
            return;
        }
        let mut file = self.file.lock().unwrap();
        let mut refill_buf = self.refill_buf.lock().unwrap();
        let mut read = self.fill_len.write().unwrap();
        let mut to_read: usize = 1000;
        if to_read + *read > (file.info.frames as usize) {
            to_read = file.info.frames as usize - *read;
        }
        println!("{} {} {}", to_read, file.info.frames, *read);
        if to_read == 0 {
            return;
        }
        assert!(refill_buf.len() == file.info.channels as usize);
        for n in 0..to_read {
            assert!(file.into_slice_float(&mut refill_buf, 1).unwrap() == 1);
            for i in 0..(file.info.channels as usize) {
                vecs[i][*read + n] = refill_buf[i];
            }
        }
        *read = *read + to_read;
    }
    pub fn new(file: SndFile) -> Vec<Self> {
        let n_chans = file.info.channels as usize;
        let n_frames = file.info.frames as usize;
        let sample_rate = file.info.samplerate as u64;
        let lp = LiveParameters {
            vol: 1.0,
            pos: 0
        };
        let mut fs = FileStream {
            file: Arc::new(Mutex::new(file)),
            refill_buf: Arc::new(Mutex::new(Vec::with_capacity(n_chans))),
            buf: Arc::new(RwLock::new(Vec::with_capacity(n_chans))),
            refilling: Arc::new(Mutex::new(())),
            fill_len: Arc::new(RwLock::new(0)),
            sample_rate: sample_rate,
            id: 0,
            lp: lp,
            controller: None
        };
        for _ in 0..n_chans {
            fs.refill_buf.lock().unwrap().push(0.0);
            fs.buf.write().unwrap().push((0..n_frames).map(|_| 0.0).collect());
        }
        fs.refill();
        let mut fs_vec = vec![fs];
        for id in 1..n_chans {
            let lp = LiveParameters {
                vol: 1.0,
                pos: 0
            };
            let fs = FileStream {
                file: fs_vec[0].file.clone(),
                refill_buf: fs_vec[0].refill_buf.clone(),
                buf: fs_vec[0].buf.clone(),
                refilling: fs_vec[0].refilling.clone(),
                fill_len: fs_vec[0].fill_len.clone(),
                sample_rate: fs_vec[0].sample_rate,
                lp: lp,
                id: id,
                controller: None
            };
            fs_vec.push(fs);
        }
        fs_vec
    }
    pub fn attach(&mut self, ctrlr: Box<StreamController + 'a>) {
        self.controller = Some(ctrlr);
        self.controller.as_mut().unwrap().init(self.lp);
    }
}
impl<'a> mixer::Mixable for FileStream<'a> {
    fn callback(&mut self, buffer: &mut [f32], frames: usize) -> mixer::CallbackResult {
        let refill = {
            //println!("Readlocking (1)...");
            if *self.fill_len.read().unwrap() < (self.lp.pos + frames) {
                true
            }
            else {
                false
            }
        };
        //println!("Readlock (1) released");
        if refill {
            println!("Attempting a refill...");
            self.refill();
        }
        //println!("Readlocking (2)...");
        let read_lck = self.buf.read().unwrap();
        if *self.fill_len.read().unwrap() < (self.lp.pos + frames) {
            return mixer::CallbackResult::None;
        }
        let buf = &read_lck[self.id];
        println!("{} {} {}", buf.len(), self.lp.pos, frames);
        let mut data = buf.split_at(self.lp.pos).1;
        if data.len() > self.lp.pos + frames {
            data = data.split_at(self.lp.pos + frames).0;
        }
        for (input, output) in data.iter().zip(buffer.iter_mut()) {
            *output = input * self.lp.vol;
        }
        self.lp.pos += frames;
        mixer::CallbackResult::More
    }
    fn control(&mut self, time: Duration) -> mixer::ControlResult {
        if self.controller.is_some() {
            if time > self.controller.as_ref().unwrap().accuracy() {
                if let None = self.controller.as_mut().unwrap().ctl(self.lp) {
                    self.controller = None;
                }
                return mixer::ControlResult::Done;
            }
        }
        mixer::ControlResult::Useless
    }
    fn sample_rate(&self) -> u64 {
        self.sample_rate
    }
    fn frames_hint(&mut self, _: usize) {}
}

/// A `StreamController` that fades in or out.
#[derive(Debug)]
pub struct FadeController {
    fade: (f32, f32),
    spec: u64,
    time: u64,
    fade_per_cnt: f32
}
impl FadeController {
    /// Make a new FadeController.
    ///
    /// - `fade(a,b)` fades from `a`dB to `b`dB.
    /// - `spec` controls how precise the fade is (how often it
    ///    alters the volume).
    /// - `time` controls how long the fade lasts for.
    pub fn new(fade: (f32, f32), spec: u64, time: u64) -> Self {
        let mut fc = FadeController {
            fade: fade,
            spec: spec,
            time: time,
            fade_per_cnt: 0.0
        };
        let diff = fade.1 - fade.0;
        let cnts = fc.time / fc.spec;
        fc.fade_per_cnt = diff / cnts as f32;
        println!("{:?}", fc);
        fc
    }
}

impl StreamController for FadeController {
    fn accuracy(&self) -> Duration {
        Duration::from_millis(self.spec)
    }
    fn init(&mut self, mut last: LiveParameters) -> LiveParameters {
        last.vol = db_lin(self.fade.0);
        last
    }
    fn ctl(&mut self, mut last: LiveParameters) -> Option<LiveParameters> {
        if last.vol <= db_lin(self.fade.1) {
            if last.vol == 0.0 {
                last.vol = db_lin(self.fade.0);
            }
            last.vol *= db_lin(self.fade_per_cnt);
        }
        else {
            return None;
        }
        Some(last)
    }
}
