//! Extraction of audio from files, and control of the resulting stream.

use rsndfile::{SndFile, SndFileInfo};
use std::sync::{Arc, RwLock, Mutex};
use std::io::{Seek, SeekFrom};
use mixer;
use uuid::Uuid;
use std::thread;
use std::sync::mpsc;
use crossbeam::sync::MsQueue;
use std::ops::Rem;
use mixer::FRAMES_PER_CALLBACK;

/// Converts a linear amplitude to decibels.
pub fn lin_db(lin: f32) -> f32 {
    lin.log10() * 20.0
}
/// Converts a decibel value to a linear amplitude.
pub fn db_lin(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}
/// Contains information about a stream.
#[derive(Debug, Clone, Copy)]
pub struct LiveParameters {
    /// Current volume (linear amplitude)
    pub vol: f32,
    /// Amount of frames written so far.
    pos: usize,
    /// Whether this stream is active (playing)
    active: bool,
    /// The start position of this stream, in frames past the start of the file.
    start: u64,
    /// The end position of this stream.
    end: u64
}
impl LiveParameters {
    /// Make a new set of LiveParameters about a file with a given start and end position.
    pub fn new(start: u64, end: u64) -> Self {
        LiveParameters {
            vol: 1.0,
            pos: 0,
            active: true,
            start: start,
            end: end
        }
    }
}

enum SpoolerCtl {
    Seek(u64)
}
/// Controller struct for a `FileStream`.
///
/// See the FileStream documentation for info about what these
/// fields are.
pub struct FileStreamX {
    lp: Arc<Mutex<LiveParameters>>,
    run: Arc<RwLock<bool>>,
    tx: mpsc::Sender<SpoolerCtl>,
    uuid: Uuid
}
impl FileStreamX {
    /// Resets the FileStream to a given position.
    pub fn reset_pos(&mut self, pos: u64) {
        self.tx.send(SpoolerCtl::Seek(pos));
    }
    /// Starts playing the FileStream from the beginning.
    pub fn start(&mut self) {
        self.reset_pos(0);
        *self.run.write().unwrap() = true;
    }
    /// Pauses the FileStream.
    pub fn pause(&mut self) {
        *self.run.write().unwrap() = false;
    }
    /// Resumes the FileStream.
    ///
    /// Similar to, but legally distinct from, the `start` function, which will
    /// reset the stream's position - whereas this just sets the stream as active and
    /// calls it good.
    pub fn unpause(&mut self) {
        *self.run.write().unwrap() = true;
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
struct FileStreamSpooler {
    file: SndFile,
    pos: usize,
    rx: mpsc::Receiver<SpoolerCtl>,
    splitting_buf: Vec<f32>,
    chan_bufs: Vec<Vec<f32>>,
    chans: Vec<Arc<MsQueue<(usize, Vec<f32>)>>>
}
impl FileStreamSpooler {
    fn new(file: SndFile, rx: mpsc::Receiver<SpoolerCtl>) -> (Self, Vec<Arc<MsQueue<(usize, Vec<f32>)>>>) {
        let mut chans = Vec::with_capacity(file.info.channels as usize);
        let mut cbs = Vec::with_capacity(file.info.channels as usize);
        let mut sb = Vec::with_capacity(file.info.channels as usize);
        for _ in 0..(file.info.channels as usize) {
            chans.push(Arc::new(MsQueue::new()));
            sb.push(0.0);
            cbs.push((0..FRAMES_PER_CALLBACK).into_iter().map(|_| 0.0).collect());
        }
        let cc = chans.clone();
        (FileStreamSpooler {
            file: file,
            rx: rx,
            pos: 0,
            chans: chans,
            splitting_buf: sb,
            chan_bufs: cbs
        }, cc)
    }
    fn reset_self(&mut self) {
        let mut run = true;
        while run {
            for i in 0..(self.file.info.channels as usize) {
                run = self.chans[i].try_pop().is_some();
            }
        }
    }
    fn handle(&mut self, msg: SpoolerCtl) {
        match msg {
            SpoolerCtl::Seek(to) => {
                assert!(self.file.seek(SeekFrom::Start(to)).unwrap() == to);
                self.reset_self();
                self.pos = to as usize;
            }
        }
    }
    fn spool(&mut self) {
        loop {
            if let Ok(msg) = self.rx.try_recv() {
                self.handle(msg);
            }
            let mut to_read: usize = 1000;
            if (to_read + self.pos) > self.file.info.frames as usize {
                to_read = self.file.info.frames as usize - self.pos;
            }
            if to_read == 0 {
                let msg = self.rx.recv().unwrap();
                self.handle(msg);
                continue;
            }
            let mut start = false;
            for n in 0..to_read {
                assert!(self.file.read_into_fslice(&mut self.splitting_buf) == 1);
                let send = n.rem(FRAMES_PER_CALLBACK) == 0 && start;
                start = true;
                for i in 0..(self.file.info.channels as usize) {
                    /*
                     * Fun fact: this if statement used to be AFTER the line after it,
                     * meaning that you would hear annoying clicks and pops when audio
                     * was played back.
                     *
                     * This was a nightmare to debug.
                     */
                    if send {
                        self.chans[i].push((self.pos + n, self.chan_bufs[i].clone()));
                    }
                    self.chan_bufs[i][n.rem(FRAMES_PER_CALLBACK)] = self.splitting_buf[i];
                }
            }
            for i in 0..(self.file.info.channels as usize) {
                self.chans[i].push((self.pos + to_read, self.chan_bufs[i].clone()));
            }
            self.pos += to_read;
        }
    }
}


/// One-channel stream created from a multi-channel file that reads from the file as it plays.
///
/// Multiple interlinked FileStreams will usually be created from the same file.
pub struct FileStream {
    buf: Arc<MsQueue<(usize, Vec<f32>)>>,
    info: SndFileInfo,
    /// The sample rate of the underlying file.
    sample_rate: u64,
    /// This FileStream's LiveParameters.
    lp: Arc<Mutex<LiveParameters>>,
    run: Arc<RwLock<bool>>,
    spooler_tx: mpsc::Sender<SpoolerCtl>,
    uuid: Uuid
}
impl FileStream {
    /// Makes a new set of FileStreams, one for each channel, from a given file.
    pub fn new(file: SndFile) -> Vec<Self> {
        let n_chans = file.info.channels as usize;
        let n_frames = file.info.frames as u64;
        let sample_rate = file.info.samplerate as u64;
        let lp = LiveParameters::new(0, n_frames);
        let sfi = file.info.clone();
        let (stx, srx) = mpsc::channel();
        let (mut spooler, qvec) = FileStreamSpooler::new(file, srx);
        let mut qvec = qvec.into_iter();
        thread::spawn(move || {
            spooler.spool();
            println!("Spooler quit");
        });

        let fs = FileStream {
            buf: qvec.next().unwrap(),
            sample_rate: sample_rate,
            lp: Arc::new(Mutex::new(lp)),
            run: Arc::new(RwLock::new(true)),
            uuid: Uuid::new_v4(),
            spooler_tx: stx,
            info: sfi
        };
        let mut fs_vec = vec![fs];
        for _ in 1..n_chans {
            let lp = LiveParameters::new(0, n_frames);
            let fs = FileStream {
                buf: qvec.next().unwrap(),
                sample_rate: fs_vec[0].sample_rate,
                lp: Arc::new(Mutex::new(lp)),
                run: fs_vec[0].run.clone(),
                uuid: Uuid::new_v4(),
                info: fs_vec[0].info.clone(),
                spooler_tx: fs_vec[0].spooler_tx.clone()
            };
            fs_vec.push(fs);
        }
        fs_vec
    }
    /// Gets an associated `FileStreamX` for this stream.
    pub fn get_x(&self) -> FileStreamX {
        FileStreamX {
            lp: self.lp.clone(),
            run: self.run.clone(),
            tx: self.spooler_tx.clone(),
            uuid: self.uuid.clone()
        }
    }
}

impl mixer::Source for FileStream {
    fn callback(&mut self, buffer: &mut [f32], frames: usize) {
        let mut lp = self.lp.lock().unwrap();
        if let Ok(r) = self.run.try_read() {
            lp.active = *r;
        }
        if lp.active == false {
            mixer::fill_with_silence(buffer);
            return;
        }
        if let Some((pos, buf)) = self.buf.try_pop() {
            assert!(buf.len() == frames);
            for (out, inp) in buffer.iter_mut().zip(buf.into_iter()) {
                *out = inp * lp.vol;
            }
            lp.pos = pos;
            if pos >= lp.end as usize {
                println!("Ended");
                lp.active = false;
            }
        }
        else {
            mixer::fill_with_silence(buffer);
        }
    }
    fn sample_rate(&self) -> u64 {
        self.sample_rate
    }
    fn frames_hint(&mut self, _: usize) {}
    fn uuid(&self) -> Uuid {
        self.uuid.clone()
    }
}
