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
use bounded_spsc_queue;
use bounded_spsc_queue::{Producer, Consumer};

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
    pub pos: usize,
    /// Whether this stream is active (playing)
    pub active: bool,
    /// The start position of this stream, in frames past the start of the file.
    pub start: u64,
    /// The end position of this stream.
    pub end: u64
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
    lp: Arc<RwLock<LiveParameters>>,
    tx: mpsc::Sender<SpoolerCtl>,
    uuid: Uuid
}
impl FileStreamX {
/*    pub fn set_fader(&mut self, fader: Box<Fn(usize, &mut f32, &mut bool) -> bool>) {
        *self.fader.write().unwrap() = Some(fader);
    }
    pub fn is_fading(&self) -> bool {
        self.fader.read().unwrap().is_some()
    }
    /// Resets the FileStream to a given position.
    pub fn reset_pos(&mut self, pos: u64) {
        self.tx.send(SpoolerCtl::Seek(pos));
    }
    /// Resets the FileStream to its start position.
    pub fn reset(&mut self) {
        self.reset_pos(0);
    }
    /// Starts playing the FileStream from the beginning.
    pub fn start(&mut self) {
        self.reset();
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
    }*/
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }
}
struct FileStreamSpooler {
    file: SndFile,
    pos: usize,
    rx: mpsc::Receiver<SpoolerCtl>,
    splitting_buf: Vec<f32>,
    chan_bufs: Vec<Vec<f32>>,
    chans: Vec<(Producer<(usize, Vec<f32>)>, Producer<FileStreamRequest>)>,
    statuses: Vec<(Consumer<LiveParameters>, Arc<RwLock<LiveParameters>>)>
}
impl FileStreamSpooler {
    fn new(file: SndFile, chans: Vec<(Producer<(usize, Vec<f32>)>, Producer<FileStreamRequest>)>, statuses: Vec<(Consumer<LiveParameters>, Arc<RwLock<LiveParameters>>)>, rx: mpsc::Receiver<SpoolerCtl>) -> Self {
        let mut cbs = Vec::with_capacity(file.info.channels as usize);
        let mut sb = Vec::with_capacity(file.info.channels as usize);
        for _ in 0..(file.info.channels as usize) {
            sb.push(0.0);
            cbs.push((0..FRAMES_PER_CALLBACK).into_iter().map(|_| 0.0).collect());
        }
        FileStreamSpooler {
            file: file,
            rx: rx,
            pos: 0,
            chans: chans,
            statuses: statuses,
            splitting_buf: sb,
            chan_bufs: cbs
        }
    }
    fn reset_self(&mut self) {
        for &mut (ref mut prod, ref mut tx) in self.chans.iter_mut() {
            let (new_tx, new_rx) = bounded_spsc_queue::make(250);
            *prod = new_tx;
            tx.push(FileStreamRequest::NewBuf(new_rx));
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
        'spooler: loop {
            if let Ok(msg) = self.rx.try_recv() {
                self.handle(msg);
            }
            let mut to_read: usize = mixer::FRAMES_PER_CALLBACK;
            if (to_read + self.pos) > self.file.info.frames as usize {
                to_read = self.file.info.frames as usize - self.pos;
            }
            if to_read == 0 {
                let msg = self.rx.recv().unwrap();
                self.handle(msg);
                continue;
            }
            for n in 0..to_read {
                assert!(self.file.read_into_fslice(&mut self.splitting_buf) == 1);
                for i in 0..(self.file.info.channels as usize) {
                    self.chan_bufs[i][n] = self.splitting_buf[i];
                }
            }
            for i in 0..(self.file.info.channels as usize) {
                self.chans[i].0.push((self.pos + to_read, self.chan_bufs[i].clone()));
            }
            self.pos += to_read;
        }
    }
}

enum FileStreamRequest {
    NewBuf(Consumer<(usize, Vec<f32>)>),
    SetVol(f32),
    SetActive(bool)
}
/// One-channel stream created from a multi-channel file that reads from the file as it plays.
///
/// Multiple interlinked FileStreams will usually be created from the same file.
pub struct FileStream {
    status_tx: Producer<LiveParameters>,
    control_rx: Consumer<FileStreamRequest>,
    buf: Consumer<(usize, Vec<f32>)>,
    lp: LiveParameters,
    info: SndFileInfo,
    sample_rate: u64,
    uuid: Uuid
}
impl FileStream {
    /// Makes a new set of FileStreams, one for each channel, from a given file.
    pub fn new(file: SndFile) -> Vec<(Self, FileStreamX)> {
        let n_chans = file.info.channels as usize;
        let n_frames = file.info.frames as u64;
        let sample_rate = file.info.samplerate as u64;

        let lp = LiveParameters::new(0, n_frames);
        let sfi = file.info.clone();
        let (spooler_ctl_tx, spooler_ctl_rx) = mpsc::channel();
        let mut streams = Vec::new();
        let mut spooler_chans = Vec::new();
        let mut spooler_statuses = Vec::new();
        for _ in 0..n_chans {
            let (status_tx, status_rx) = bounded_spsc_queue::make(100);
            let (buf_tx, buf_rx) = bounded_spsc_queue::make(FRAMES_PER_CALLBACK);
            let (control_tx, control_rx) = bounded_spsc_queue::make(25);
            let lp = Arc::new(RwLock::new(lp.clone()));
            let uu = Uuid::new_v4();

            spooler_chans.push((buf_tx, control_tx));
            spooler_statuses.push((status_rx, lp.clone()));

            streams.push((FileStream {
                status_tx: status_tx,
                control_rx: control_rx,
                buf: buf_rx,
                lp: LiveParameters::new(0, n_frames),
                info: sfi.clone(),
                sample_rate: sample_rate,
                uuid: uu
            }, FileStreamX {
                lp: lp,
                tx: spooler_ctl_tx.clone(),
                uuid: uu.clone()
            }));
        }
        let mut spooler = FileStreamSpooler::new(file, spooler_chans, spooler_statuses, spooler_ctl_rx);
        thread::spawn(move || {
            spooler.spool();
        });
        streams
    }
}

impl mixer::Source for FileStream {
    fn callback(&mut self, buffer: &mut [f32], _: usize, zero: bool) {
        if let Some(fsreq) = self.control_rx.try_pop() {
            match fsreq {
                FileStreamRequest::NewBuf(nb) => self.buf = nb,
                FileStreamRequest::SetVol(vol) => self.lp.vol = vol,
                FileStreamRequest::SetActive(act) => self.lp.active = act
            }
        }
        if self.lp.active == false {
            mixer::fill_with_silence(buffer);
            return;
        }
        if let Some((pos, buf)) = self.buf.try_pop() {
            for (out, inp) in buffer.iter_mut().zip(buf.into_iter()) {
                if zero {
                    *out = 0.0;
                }
                *out = *out + (inp * self.lp.vol);
            }
            self.lp.pos = pos;
            if pos >= self.lp.end as usize {
                self.lp.active = false;
                self.status_tx.try_push(self.lp.clone());
            }
            else if pos.rem(441) == 0 {
                self.status_tx.try_push(self.lp.clone());
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
