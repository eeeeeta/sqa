//! Extraction of audio from files, and control of the resulting stream.
use std::sync::Arc;
use parking_lot::{Mutex, Condvar};
use std::io::{Seek, SeekFrom};
use mixer;
use uuid::Uuid;
use std::thread;
use std::sync::mpsc;
use mixer::FRAMES_PER_CALLBACK;
use bounded_spsc_queue;
use bounded_spsc_queue::{Producer, Consumer};
use backend::BackendSender;
use state::Message;
use command;
use commands::LoadCommand;
use std::time::Duration;
use simplemad::{Decoder, Frame, SimplemadError};
use std::fs::File;
use std::io::{self, BufReader, ErrorKind};
use std::path::{Path, PathBuf};

/* FIXME(for this entire file): give some notification on try_push failures */
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
    /// The duration past the start of the file of the last frame played.
    pub pos: Duration,
    /// Whether this stream is active (playing)
    pub active: bool,
    /// Whether this stream has come to its end (stopped) and is still there
    pub stopped: bool,
    /// The duration to start playing at.
    pub start: Duration,
    /// The duration past which the stream will stop playing.
    pub end: Duration
}
impl LiveParameters {
    /// Make a new set of LiveParameters about a file with a given start and end position.
    pub fn new(start: Duration, end: Duration) -> Self {
        LiveParameters {
            vol: 1.0,
            pos: Duration::new(0, 0),
            active: false,
            stopped: false,
            start: start,
            end: end
        }
    }
}

enum SpoolerCtl {
    Seek(u64),
    SetVol(f32),
    SetActive(bool),
}
/// Controller struct for a `FileStream`.
///
/// See the FileStream documentation for info about what these
/// fields are.
#[derive(Clone)]
pub struct FileStreamX {
    tx: mpsc::Sender<SpoolerCtl>,
    cvar: Arc<(Mutex<()>, Condvar)>,
    uuid: Uuid
}
impl FileStreamX {
    fn send(&mut self, s: SpoolerCtl) {
        self.tx.send(s).unwrap();
        self.cvar.1.notify_one();
    }
    /// Resets the FileStream to a given position.
    pub fn reset_pos(&mut self, pos: u64) {
        self.send(SpoolerCtl::Seek(pos));
    }
    /// Resets the FileStream to its start position.
    pub fn reset(&mut self) {
        self.reset_pos(0);
    }
    /// Starts playing the FileStream from the beginning.
    pub fn start(&mut self) {
        self.reset();
        self.send(SpoolerCtl::SetActive(true));
    }
    /// Pauses the FileStream.
    pub fn pause(&mut self) {
        self.send(SpoolerCtl::SetActive(false));
    }
    /// Stops the FileStream.
    pub fn stop(&mut self) {
        self.send(SpoolerCtl::SetActive(false));
        self.send(SpoolerCtl::Seek(0));
    }
    /// Resumes the FileStream.
    ///
    /// Similar to, but legally distinct from, the `start` function, which will
    /// reset the stream's position - whereas this just sets the stream as active and
    /// calls it good.
    pub fn unpause(&mut self) {
        self.send(SpoolerCtl::SetActive(true));
    }
    /// Sets the volume of the FileStream.
    pub fn set_vol(&mut self, vol: f32) {
        self.send(SpoolerCtl::SetVol(vol));
    }
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }
}
struct ChannelDescriptor {
    buf_tx: Producer<(Duration, Vec<f32>)>,
    ctl_tx: Producer<FileStreamRequest>,
    lp_rx: Consumer<LiveParameters>,
    bufs: Vec<(Duration, Vec<f32>)>
}
struct FileStreamSpooler {
    decoder: Decoder<File>,
    notifier: (Uuid, BackendSender),
    rx: mpsc::Receiver<SpoolerCtl>,
    chans: Vec<ChannelDescriptor>,
    pos: Duration,
    eof: bool,
    filled: bool,
    cvar: Arc<(Mutex<()>, Condvar)>
}
impl FileStreamSpooler {
    fn new(dec: Decoder<File>, not: (Uuid, BackendSender), rx: mpsc::Receiver<SpoolerCtl>, chans: Vec<ChannelDescriptor>, cvar: Arc<(Mutex<()>, Condvar)>) -> Self {
        FileStreamSpooler {
            decoder: dec,
            notifier: not,
            rx: rx,
            chans: chans,
            pos: Duration::new(0, 0),
            eof: false,
            filled: false,
            cvar: cvar
        }
    }
    fn reset_self(&mut self) {
        for &mut ChannelDescriptor { ref mut buf_tx, ref mut ctl_tx, ref mut bufs, .. }in self.chans.iter_mut() {
            let (new_tx, new_rx) = bounded_spsc_queue::make(250);
            *buf_tx = new_tx;
            *bufs = vec![(Duration::new(0, 0), Vec::with_capacity(FRAMES_PER_CALLBACK))];
            ctl_tx.push(FileStreamRequest::NewBuf(new_rx));
        }
        self.pos = Duration::new(0, 0);
    }
    fn handle(&mut self, msg: SpoolerCtl) {
        match msg {
            SpoolerCtl::Seek(to) => {
                assert!(self.decoder.get_reader().seek(SeekFrom::Start(to)).unwrap() == to);
                self.decoder.zero_buffer().unwrap();
                self.reset_self();
                self.eof = false;
                self.filled = false;
            },
            SpoolerCtl::SetActive(act) => {
                for &mut ChannelDescriptor { ref mut ctl_tx, .. } in self.chans.iter_mut() {
                    ctl_tx.push(FileStreamRequest::SetActive(act));
                }
            },
            SpoolerCtl::SetVol(vol) => {
                for &mut ChannelDescriptor { ref mut ctl_tx, .. } in self.chans.iter_mut() {
                    ctl_tx.push(FileStreamRequest::SetVol(vol));
                }
            }
        }
    }
    fn send_buffers(buf_tx: &mut Producer<(Duration, Vec<f32>)>, bufs: &mut Vec<(Duration, Vec<f32>)>) {
        let mut len = bufs.len();
        if len < 2 || buf_tx.size() == buf_tx.capacity() {
            return;
        }
        len = buf_tx.capacity() - buf_tx.size();
        if len > (bufs.len() - 1) {
            len = bufs.len() - 1;
        }
        bufs.reverse(); // reversed, so bufs[0] is at the end & filled bufs at front
        for buf in bufs.drain(0..len) {
            assert!(buf.1.len() == buf.1.capacity());
            buf_tx.push(buf);
        }
        bufs.reverse();
    }
    fn spool(&mut self) {
        'spooler: loop {
            if let Ok(msg) = self.rx.try_recv() {
                self.handle(msg);
            }
            for (i, &mut ChannelDescriptor { ref mut lp_rx, ref mut bufs, ref mut buf_tx, .. }) in self.chans.iter_mut().enumerate() {
                let mut lp = None;
                while let Some(stat) = lp_rx.try_pop() {
                    lp = Some(stat);
                }
                if let Some(lp) = lp {
                    if buf_tx.size() * 2 < buf_tx.capacity() {
                        self.filled = false;
                    }
                    self.notifier.1.send(Message::Update(
                        self.notifier.0,
                        command::new_update(move |cmd: &mut LoadCommand| {
                            cmd.streams[i].lp = lp;
                            lp.stopped
                        })
                    )).unwrap();
                }
                Self::send_buffers(buf_tx, bufs);
            }
            if self.eof == true || self.filled == true {
                self.cvar.1.wait(&mut self.cvar.0.lock());
                continue 'spooler;
            }
            match self.decoder.get_frame() {
                Err(SimplemadError::Read(err)) => {
                    println!("ERROR: Reader errored: {:?}", err);
                    self.eof = true;
                    continue 'spooler;
                },
                Err(SimplemadError::Mad(_)) => {
                    continue;
                }
                Err(SimplemadError::EOF) => {
                    for &mut ChannelDescriptor { ref mut bufs, .. } in self.chans.iter_mut() {
                        while bufs[0].1.len() != bufs[0].1.capacity() {
                            bufs[0].1.push(0.0);
                        }
                        bufs[0].0 = self.pos;
                        bufs.insert(0, (self.pos, Vec::with_capacity(FRAMES_PER_CALLBACK)));
                    }
                    self.eof = true;
                    continue 'spooler;
                },
                Ok(frame) => {
                    // # How the filling of the buffers works
                    //
                    // Each channel has, in its ChannelDescriptor, a Vec of (dur, vec) pairs
                    // where dur = position past file start of last sample in vec, and vec =
                    // `mixer::FRAMES_PER_CALLBACK` f32 samples at most.
                    //
                    // We always keep a not-yet-full buffer in bufs[0], which is where new buffers
                    // are put. Other filled buffers are shifted off to the right.
                    //
                    // When adding a new sample: we first check whether bufs[0] is full, and if so,
                    // we amend its duration to our current position and insert a new buf in pos 0.
                    // Then, the sample is pushed into bufs[0].
                    //
                    // After all samples are added, bufs[0] is checked to see whether it is full,
                    // and given the same treatment as in the sample addition loop. This protects
                    // against the case where buf[0] is filled perfectly at the end of sample
                    // addition, and is thus treated as not-yet-full and isn't pushed to the audio
                    // callback.
                    //
                    // Then, we drain all the full buffers, starting from the end (ones that aren't
                    // bufs[0]) and send them.
                    for (ch, samples) in frame.samples.into_iter().enumerate() {
                        if self.chans.get(ch).is_none() {
                            println!("WARNING: extra channel {} decoded", ch);
                            continue;
                        }
                        let ChannelDescriptor {
                            ref mut bufs,
                            ref mut buf_tx, .. } = self.chans[ch];

                        if bufs.get(0).is_none() { bufs.insert(0, (self.pos, Vec::with_capacity(FRAMES_PER_CALLBACK))); }
                        let len = samples.len();
                        for (i, smpl) in samples.into_iter().enumerate() {
                            if bufs[0].1.len() == bufs[0].1.capacity() {
                                bufs[0].0 = self.pos + ((frame.duration / len as u32) * i as u32);
                                bufs.insert(0, (self.pos, Vec::with_capacity(FRAMES_PER_CALLBACK)));
                            }
                            bufs[0].1.push(smpl.to_f32());
                        }
                        if bufs[0].1.len() == bufs[0].1.capacity() {
                            bufs[0].0 = self.pos + frame.duration;
                            bufs.insert(0, (self.pos, Vec::with_capacity(FRAMES_PER_CALLBACK)));
                        }
                        if buf_tx.size() == buf_tx.capacity() {
                            self.filled = true;
                            self.pos += frame.duration;
                            continue 'spooler;
                        }
                        Self::send_buffers(buf_tx, bufs);
                    }
                    self.pos += frame.duration;
                }
            }
        }
    }
}
enum FileStreamRequest {
    NewBuf(Consumer<(Duration, Vec<f32>)>),
    SetVol(f32),
    SetActive(bool)
}
/// One-channel stream created from a multi-channel file that reads from the file as it plays.
///
/// Multiple interlinked FileStreams will usually be created from the same file.
pub struct FileStream {
    status_tx: Producer<LiveParameters>,
    control_rx: Consumer<FileStreamRequest>,
    buf: Consumer<(Duration, Vec<f32>)>,
    lp: LiveParameters,
    sample_rate: u32,
    uuid: Uuid,
    last_sec: u64,
    cvar: Arc<(Mutex<()>, Condvar)>
}
pub struct FileInfo {
    pub n_chans: usize,
    pub sample_rate: u32,
    pub duration: Duration
}
impl FileStream {
    pub fn info(filename: &Path) -> Result<FileInfo, io::Error> {
        let file = File::open(filename)?;
        let mut decoder = Decoder::decode(file).map_err(|e| {
            match e {
                SimplemadError::Read(e) => e,
                SimplemadError::Mad(e) => io::Error::new(ErrorKind::Other,
                                                         format!("MP3 decoder error: {:?}", e)),
                SimplemadError::EOF => io::Error::new(ErrorKind::UnexpectedEof,
                                                      format!("Unexpected EOF"))
            }
        })?;
        let mut frame: Option<Frame> = None;
        for ret in &mut decoder {
            match ret {
                Err(SimplemadError::Read(e)) => {
                    return Err(e);
                },
                Err(SimplemadError::Mad(_)) => {
                    continue;
                }
                Err(SimplemadError::EOF) => {
                    return Err(io::Error::new(ErrorKind::UnexpectedEof,
                                              format!("File ended before any useful data was decoded")));
                },
                Ok(fr) => {
                    frame = Some(fr);
                    break;
                }
            }
        }
        let frame = match frame {
            Some(fr) => fr,
            None => return Err(io::Error::new(ErrorKind::UnexpectedEof,
                                              format!("File ended before any useful data was decoded")))
        };

        decoder.get_reader().seek(SeekFrom::Start(0))?;
        decoder.zero_buffer()?;
        decoder.set_headers_only(true);

        let duration = decoder.filter_map(|r| {
            match r {
                Ok(f) => Some(f.duration),
                Err(_) => None,
            }
        }).fold(Duration::new(0, 0), |acc, dtn| acc + dtn);

        Ok(FileInfo {
            n_chans: frame.samples.len(),
            sample_rate: frame.sample_rate,
            duration: duration
        })
    }
    /// Makes a new set of FileStreams, one for each channel, from a given file.
    pub fn new(filename: ::std::path::PathBuf, channel: BackendSender, auuid: Uuid) -> Result<Vec<(Self, FileStreamX)>, io::Error> {
        let info = Self::info(filename.as_path())?;

        let lp = LiveParameters::new(Duration::new(0, 0), info.duration);
        let (spoolertx, spoolerrx) = mpsc::channel();
        let cvar = Arc::new((Mutex::new(()), Condvar::new()));
        let mut streams = Vec::new();
        let mut descriptors = Vec::new();
        for _ in 0..info.n_chans {
            let (status_tx, status_rx) = bounded_spsc_queue::make(100);
            let (buf_tx, buf_rx) = bounded_spsc_queue::make(FRAMES_PER_CALLBACK);
            let (control_tx, control_rx) = bounded_spsc_queue::make(25);
            let uu = Uuid::new_v4();

            descriptors.push(ChannelDescriptor {
                buf_tx: buf_tx,
                ctl_tx: control_tx,
                lp_rx: status_rx,
                bufs: Vec::with_capacity(5)
            });
            streams.push((FileStream {
                status_tx: status_tx,
                control_rx: control_rx,
                buf: buf_rx,
                lp: lp.clone(),
                sample_rate: info.sample_rate,
                uuid: uu,
                last_sec: 0,
                cvar: cvar.clone()
            }, FileStreamX {
                tx: spoolertx.clone(),
                uuid: uu.clone(),
                cvar: cvar.clone()
            }));
        }
        thread::spawn(move || {
            let mut file = File::open(filename.as_path()).unwrap();
            let mut decoder = Decoder::decode(file).unwrap();

            let mut spooler = FileStreamSpooler::new(decoder, (auuid, channel), spoolerrx, descriptors, cvar);
            spooler.spool();
        });
        Ok(streams)
    }
}

impl mixer::Source for FileStream {
    fn callback(&mut self, buffer: &mut [f32], _: usize, zero: bool) {
        if let Some(fsreq) = self.control_rx.try_pop() {
            match fsreq {
                FileStreamRequest::NewBuf(nb) => {
                    self.buf = nb;
                    self.last_sec = 0;
                },
                FileStreamRequest::SetVol(vol) => self.lp.vol = vol,
                FileStreamRequest::SetActive(act) =>
                {
                    self.lp.active = act;
                    if act == true {
                        self.lp.stopped = false;
                    }
                }
            }
            self.cvar.1.notify_one();
            self.status_tx.try_push(self.lp.clone());
        }
        if self.lp.active == false {
            mixer::fill_with_silence(buffer, zero);
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
            if pos >= self.lp.end {
                self.lp.active = false;
                self.lp.stopped = true;
                self.lp.pos = Duration::new(0, 0);
                self.status_tx.try_push(self.lp.clone());
                self.cvar.1.notify_one();
            }
            else if pos.as_secs() > self.last_sec {
                self.last_sec = pos.as_secs();
                self.status_tx.try_push(self.lp.clone());
                self.cvar.1.notify_one();
            }
        }
        else {
            mixer::fill_with_silence(buffer, zero);
        }
    }
    fn sample_rate(&self) -> u64 {
        self.sample_rate as u64 /* FIXME */
    }
    fn uuid(&self) -> Uuid {
        self.uuid.clone()
    }
}
