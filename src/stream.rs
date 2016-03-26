//! A module for controlling and playing audio streams.
//!
//! Uses clever thread magicks to be able to play many streams at once,
//! and control each.

use portaudio as pa;
use rsndfile::{SndFile, SndFileInfo};
use std::sync::mpsc::{Sender, Receiver, channel, TryRecvError};
use std::time::Duration;
use chrono::duration::Duration as CDuration;
use std::thread;
use std::sync::{Arc, Mutex};
use std::mem;
/// Controls the number of samples per PortAudio callback.
///
/// If you're experiencing underruns, raise this number. Note that
/// in theory this will result in less fine-grained control of the audio,
/// but in practice that doesn't really occur with the current code.
///
/// **Note**: if you're using PulseAudio & getting underruns, try setting the following in
/// /etc/pulse/daemon.conf first:
///
/// ```text
/// default-fragments = 5
/// default-fragment-size-msec = 2
/// ```
pub const SAMPLES_PER_CALLBACK: u32 = 50;

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
    frames_written: u64,
    /// Amount of frames to write in total.
    frames_total: u64
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
}
// Type sent to a `ThreadStream` to ask it to carry out an action.
enum ThreadAction {
    // Request to attach a `StreamController` to the stream.
    AttachController(Box<StreamController + Send>),
    ILive,
    BeginPlayback
}
/// A stream of playing music.
///
/// This is the externally accessible, nicely abstracted version, which only serves to
/// spawn, instruct, and share state with a thread containing a `ThreadStream`, which controls
/// the stream itself.
pub struct Stream {
    /// Information about the soundfile to be played
    info: SndFileInfo,
    /// What this stream currently knows about its state.
    ///
    /// This is shared with the `ThreadStream` thread and PA callback thread pertaining to this stream.
    state: Arc<Mutex<LiveParameters>>,
    /// Channel to this stream's underlying `ThreadStream` to send actions to be executed.
    ts_tx: Sender<ThreadAction>,
    ts_rx: Receiver<ThreadAction>,
    jh: Option<thread::JoinHandle<Result<(), pa::error::Error>>>
}
/// Thread-local stream that communicates with a host `Stream`.
///
/// Handles all the actual music playing, controller controlling, and the like.
struct ThreadStream<'a> {
    /// Underlying PortAudio stream.
    pa_stream: pa::stream::Stream<'a, pa::stream::NonBlocking, pa::stream::Output<f32>>,
    /// Active stream controller (if any)
    controller: Option<Box<StreamController>>,
    /// Shared state with host `Stream`
    state: Arc<Mutex<LiveParameters>>,
    /// Information about the soundfile to be played
    info: SndFileInfo,
    /// Reciever for `ThreadActions` from host `Stream`
    s_rx: Receiver<ThreadAction>,
    s_tx: Sender<ThreadAction>
}
#[derive(Debug)]
pub enum StreamError {
    PaError(pa::error::Error),
    ThreadPanicked
}

impl Stream {
    /// Make a new stream to play a file.
    pub fn new(mut file: SndFile) -> Result<Self, StreamError> {
        let (ts_tx, ts_rx) = channel();
        let (ts_tx1, ts_rx1) = channel();
        let info = file.info.clone();
        let mut stream = Stream {
            state: Arc::new(Mutex::new(LiveParameters {
                vol: 0.0,
                frames_written: 0,
                frames_total: info.frames as u64
            })),
            info: info,
            ts_tx: ts_tx,
            ts_rx: ts_rx1,
            jh: None
        };
        let stream_state = stream.state.clone();
        stream.jh = Some(thread::spawn(move || {
            let pa = try!(pa::PortAudio::new());
            let num_devices = try!(pa.device_count());
            println!("Number of devices = {}", num_devices);
            let mut def_output = try!(pa.default_output_device());
            for device in try!(pa.devices()) {
                let (idx, info) = try!(device);
                println!("--------------------------------------- {:?}", idx);
                println!("{:#?}", &info);
                if info.name.contains("system") && info.host_api == 2 {
                    println!("is JACK");
                    def_output = idx;
                    break;
                }
            }
            let output_info = try!(pa.device_info(def_output));
            let output_params: pa::StreamParameters<f32> = pa::StreamParameters::new(def_output, file.info.channels, true, output_info.default_low_output_latency);
            try!(pa.is_output_format_supported(output_params, file.info.samplerate as f64));
            let settings = pa::stream::OutputSettings::new(output_params, file.info.samplerate as f64, SAMPLES_PER_CALLBACK);

            let mut this_state = LiveParameters {
                vol: 0.0,
                frames_written: 0,
                frames_total: file.info.frames as u64
            };
            let file_info = file.info.clone();
            let callback_arc = stream_state.clone();
            let callback = move |pa::stream::OutputCallbackArgs { buffer, frames, .. }| {
                let written = file.into_slice_float(buffer, frames).unwrap();
                let state = callback_arc.try_lock();
                if state.is_ok() {
                    this_state.vol = state.as_ref().unwrap().vol;
                }
                for smpl in buffer.iter_mut() {
                    *smpl = *smpl * this_state.vol;
                }
                if written < frames {
                    return pa::Complete;
                }
                this_state.frames_written += written as u64;
                if state.is_ok() {
                    *state.unwrap() = this_state;
                }
                if this_state.frames_total >= this_state.frames_written { pa::Continue } else { pa::Complete }
            };

            let mut thread_stream = ThreadStream {
                pa_stream: try!(pa.open_non_blocking_stream(settings, callback)),
                controller: None,
                state: stream_state.clone(),
                info: file_info,
                s_rx: ts_rx,
                s_tx: ts_tx1
            };
            thread_stream.check();
            thread_stream.run();
            Ok(())
        }));
        // Block until stream is usable.
        let msg = stream.ts_rx.recv();
        if let Ok(_) = msg {
            Ok(stream)
        }
        else {
            let join_result = stream.jh.take().unwrap().join();
            if let Ok(res) = join_result {
                Err(StreamError::PaError(res.unwrap_err())) /* if this is Ok, something's gone terribly wrong */
            }
            else {
                Err(StreamError::ThreadPanicked)
            }
        }

    }
    /// Start playing a stream, including initialisation of audio.
    ///
    /// Takes a mutex to a PortAudio instance, though in practice I don't think
    /// sharing those works out.
    pub fn run(&mut self) -> thread::JoinHandle<Result<(), pa::error::Error>> {
        self.ts_tx.send(ThreadAction::BeginPlayback).unwrap();
        self.jh.take().unwrap()
    }
    /// Attach a controller to this stream.
    ///
    /// Currently just overwrites the existing one, if present.
    ///
    /// FIXME: handle Result
    pub fn attach(&mut self, sc: Box<StreamController + Send>) {
        self.ts_tx.send(ThreadAction::AttachController(sc));
    }

}
impl<'a> ThreadStream<'a> {
    pub fn check(&mut self) {
        self.s_tx.send(ThreadAction::ILive);
        loop {
            let msg = self.s_rx.recv();
            if let Ok(ThreadAction::BeginPlayback) = msg {
                break;
            }
            else if let Ok(ta) = msg {
                self.handle_ta(ta);
            }
            else {
                msg.unwrap();
            }
        }
    }
    /// Starts the main thread loop - processing controllers and running them.
    pub fn run(&mut self) {
        self.pa_stream.start().unwrap();
        while let true = self.pa_stream.is_active().unwrap() {
            {
                let lck_state = self.state.lock().unwrap();
                let sel = CDuration::seconds((lck_state.frames_written / (self.info.samplerate as u64)) as i64);
                let stl = CDuration::seconds((lck_state.frames_total / (self.info.samplerate as u64)) as i64);
                let sel_hrs = sel.num_hours();
                let sel_mins = sel.num_minutes() - (sel_hrs * 60);
                let sel_secs = sel.num_seconds() - (sel_mins * 60);

                let stl_hrs = stl.num_hours();
                let stl_mins = stl.num_minutes() - (stl_hrs * 60);
                let stl_secs = stl.num_seconds() - (stl_mins * 60);
                let ca_str = if self.controller.is_some() { " (controller active)" } else { "" };
                println!("{:02}:{:02}:{:02} / {:02}:{:02}:{:02} (vol: {:.2}dB){}",
                         sel_hrs, sel_mins, sel_secs,
                         stl_hrs, stl_mins, stl_secs,
                         lin_db(lck_state.vol),
                         ca_str
                );
            }
            if let Ok(ta) = self.s_rx.try_recv() {
                self.handle_ta(ta);
            }
            if self.controller.is_some() {
                {
                    let mut state = self.state.lock().unwrap();
                    println!("controller has mutex");
                    let params = self.controller.as_mut().unwrap().ctl(state.clone());
                    if params.is_none() {
                        self.controller = None;
                        println!("thread: controller done");
                        continue;
                    }
                    *state = params.unwrap();
                }
                thread::sleep(self.controller.as_mut().unwrap().accuracy());
            }
            else {
                thread::sleep(Duration::from_millis(500));
            }
        }
        self.pa_stream.stop().unwrap();
    }
    pub fn handle_ta(&mut self, ta: ThreadAction) {
        match ta {
            ThreadAction::AttachController(ctl) => self.attach(ctl),
            _ => panic!("unexpected ThreadAction")
        }
    }
    /// Attaches a `StreamController` to this stream.
    pub fn attach(&mut self, sc: Box<StreamController>) {
        self.controller = Some(sc);
    }
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
