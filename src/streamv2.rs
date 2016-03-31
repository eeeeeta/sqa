//! Extraction of audio from files, and control of the resulting stream.

use rsndfile::SndFile;
use std::sync::{Arc, RwLock, Mutex};
use std::io::{Seek, SeekFrom};
use mixer;

/// Converts a linear amplitude to decibels.
pub fn lin_db(lin: f32) -> f32 {
    lin.log10() * 20.0
}
/// Converts a decibel value to a linear amplitude.
pub fn db_lin(db: f32) -> f32 {
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
    pos: usize,
    active: bool,
    start: u64,
    end: u64,
    ver: usize
}
impl LiveParameters {
    pub fn new(start: u64, end: u64) -> Self {
        LiveParameters {
            vol: 1.0,
            pos: 0,
            active: true,
            start: start,
            end: end,
            ver: 0
        }
    }
}
fn update_slp<'a>(olp: &'a mut Arc<Mutex<LiveParameters>>, slp: &'a mut Arc<Mutex<LiveParameters>>) -> ::std::sync::MutexGuard<'a, LiveParameters> {
    let mut lp = slp.lock().unwrap();
    let olp = olp.lock().unwrap();
    lp.pos = olp.pos;
    lp.start = olp.start;
    lp.end = olp.end;
    lp.active = olp.active;
    lp.ver = lp.ver + 1;
    lp
}
pub struct FileStreamX {
    file: Arc<Mutex<SndFile>>,
    fill_len: Arc<RwLock<usize>>,
    lp: Arc<Mutex<LiveParameters>>,
    shared_lp: Arc<Mutex<LiveParameters>>
}
impl FileStreamX {
    /// Resets the FileStream to a given position.
    pub fn reset_pos(&mut self, pos: u64) {
        let mut lp = update_slp(&mut self.lp, &mut self.shared_lp);
        let mut file = self.file.lock().unwrap();
        let mut fill_len = self.fill_len.write().unwrap();
        lp.pos = pos as usize;
        *fill_len = pos as usize;
        assert!(file.seek(SeekFrom::Start(pos)).unwrap() == pos);
    }
    /// Starts playing the FileStream from the beginning.
    pub fn start(&mut self) {
        let start_pos = self.shared_lp.lock().unwrap().start;
        self.reset_pos(start_pos);
        update_slp(&mut self.lp, &mut self.shared_lp).active = true;
    }
    /// Pauses the FileStream.
    pub fn pause(&mut self) {
        update_slp(&mut self.lp, &mut self.shared_lp).active = false;
    }
    /// Resumes the FileStream.
    ///
    /// Similar to, but legally distinct from, the `start` function, which will
    /// reset the stream's position - whereas this just sets the stream as active and
    /// calls it good.
    pub fn unpause(&mut self) {
        update_slp(&mut self.lp, &mut self.shared_lp).active = true;
    }
    /// Get this FileStream's current LiveParameters.
    pub fn lp(&self) -> LiveParameters {
        self.lp.lock().unwrap().clone()
    }
    /// Sets the volume of the FileStream.
    pub fn set_vol(&mut self, vol: f32) {
        let mut lp = self.lp.lock().unwrap();
        lp.vol = vol;
    }
}
pub struct FileStream {
    file: Arc<Mutex<SndFile>>,
    refill_buf: Arc<Mutex<Vec<f32>>>,
    buf: Arc<RwLock<Vec<Vec<f32>>>>,
    refilling: Arc<Mutex<()>>,
    fill_len: Arc<RwLock<usize>>,
    sample_rate: u64,
    id: usize,
    lp: Arc<Mutex<LiveParameters>>,
    shared_lp: Arc<Mutex<LiveParameters>>
}
impl FileStream {
    pub fn refill(&mut self, up_to: usize) {
        let write_lck = self.refilling.try_lock();
        let mut vecs: ::std::sync::RwLockWriteGuard<Vec<Vec<f32>>>;
        if let Ok(_) = write_lck {
            vecs = self.buf.write().unwrap();
        }
        else {
            return;
        }
        let mut file = self.file.lock().unwrap();
        let mut refill_buf = self.refill_buf.lock().unwrap();
        let mut read = self.fill_len.write().unwrap();
        let mut to_read: usize = 1000;
        if (to_read + *read) > (file.info.frames as usize) {
            to_read = file.info.frames as usize - *read;
        }
        if (to_read + *read) < up_to && up_to < (file.info.frames as usize) {
            *read = up_to - to_read;
            assert!(file.seek(SeekFrom::Start(up_to as u64)).unwrap() == up_to as u64);
        }

        if to_read == 0 {
            return;
        }
        assert!(refill_buf.len() == file.info.channels as usize);
        for n in 0..to_read {
            assert!(file.read_into_fslice(&mut refill_buf) == 1);
            for i in 0..(file.info.channels as usize) {
                vecs[i][*read + n] = refill_buf[i];
            }
        }
        *read = *read + to_read;
    }
    pub fn new(file: SndFile) -> Vec<Self> {
        let n_chans = file.info.channels as usize;
        let n_frames = file.info.frames as u64;
        let sample_rate = file.info.samplerate as u64;
        let lp = LiveParameters::new(0, n_frames);
        let shared_lp = LiveParameters::new(0, n_frames);
        let mut fs = FileStream {
            file: Arc::new(Mutex::new(file)),
            refill_buf: Arc::new(Mutex::new(Vec::with_capacity(n_chans))),
            buf: Arc::new(RwLock::new(Vec::with_capacity(n_chans))),
            refilling: Arc::new(Mutex::new(())),
            fill_len: Arc::new(RwLock::new(0)),
            sample_rate: sample_rate,
            id: 0,
            lp: Arc::new(Mutex::new(lp)),
            shared_lp: Arc::new(Mutex::new(shared_lp))
        };
        for _ in 0..n_chans {
            fs.refill_buf.lock().unwrap().push(0.0);
            fs.buf.write().unwrap().push((0..n_frames).map(|_| 0.0).collect());
        }
        fs.refill(0);
        let mut fs_vec = vec![fs];
        for id in 1..n_chans {
            let lp = LiveParameters::new(0, n_frames);
            let fs = FileStream {
                file: fs_vec[0].file.clone(),
                refill_buf: fs_vec[0].refill_buf.clone(),
                buf: fs_vec[0].buf.clone(),
                refilling: fs_vec[0].refilling.clone(),
                fill_len: fs_vec[0].fill_len.clone(),
                sample_rate: fs_vec[0].sample_rate,
                lp: Arc::new(Mutex::new(lp)),
                shared_lp: fs_vec[0].shared_lp.clone(),
                id: id
            };
            fs_vec.push(fs);
        }
        fs_vec
    }
    pub fn get_x(&self) -> FileStreamX {
        FileStreamX {
            file: self.file.clone(),
            fill_len: self.fill_len.clone(),
            lp: self.lp.clone(),
            shared_lp: self.shared_lp.clone()
        }
    }
}

impl mixer::Mixable for FileStream {
    fn callback(&mut self, buffer: &mut [f32], frames: usize) {
        if self.shared_lp.lock().unwrap().ver > self.lp.lock().unwrap().ver {
            println!("Updating!");
            let slp = self.shared_lp.lock().unwrap();
            let mut lp = self.lp.lock().unwrap();
            lp.active = slp.active;
            lp.pos = slp.pos;
            lp.start = slp.start;
            lp.end = slp.end;
            lp.ver = slp.ver;
        }
        if self.lp.lock().unwrap().active == false {
            mixer::fill_with_silence(buffer);
            return;
        }
        let refill = {
            if *self.fill_len.read().unwrap() < (self.lp.lock().unwrap().pos + frames) {
                true
            }
            else {
                false
            }
        };
        if refill {
            let pos = self.lp.lock().unwrap().pos + frames;
            self.refill(pos);
        }
        let mut lp = self.lp.lock().unwrap();
        let read_lck = self.buf.read().unwrap();
        if *self.fill_len.read().unwrap() < (lp.pos + frames) {
            lp.active = false;
            mixer::fill_with_silence(buffer);
            return;
        }
        let buf = &read_lck[self.id];
        let mut data = buf.split_at(lp.pos).1;
        if data.len() > lp.pos + frames {
            data = data.split_at(lp.pos + frames).0;
        }
        for (input, output) in data.iter().zip(buffer.iter_mut()) {
            *output = input * lp.vol;
        }
        lp.pos += frames;
    }
    fn sample_rate(&self) -> u64 {
        self.sample_rate
    }
    fn frames_hint(&mut self, _: usize) {}
}
