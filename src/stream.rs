//! A module for controlling and playing audio streams.

use portaudio as pa;
use rsndfile::{SndFile, SndFileInfo};
use std::sync::mpsc::{Sender, Receiver, channel};
use std::time::Duration;
use chrono::duration::Duration as CDuration;
use std::thread;

/// Controls the number of samples per PortAudio callback.
///
/// If you're experiencing underruns, raise this number. Note that
/// in theory this will result in less fine-grained control of the audio,
/// but in practice that doesn't really occur with the current code.

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
/// A stream of playing music.
pub struct Stream<'a> {
    /// Underlying PortAudio stream.
    pa_stream: pa::stream::Stream<'a, pa::stream::NonBlocking, pa::stream::Output<f32>>,
    /// What this stream currently knows about its state (note: may lag behind actual state)
    state: LiveParameters,
    /// Information about this stream's file.
    info: SndFileInfo,
    /// Sends `LiveParameters` to the stream to alter its state.
    tx: Sender<LiveParameters>,
    /// Recieves `LiveParameters` from the stream to update the `state` variable.
    rx: Receiver<LiveParameters>,
    /// A boxed `StreamController`.
    controller: Option<Box<StreamController>>
}
impl<'a> Stream<'a> {
    pub fn new(pa_s: pa::stream::Stream<'a, pa::stream::NonBlocking, pa::stream::Output<f32>>, sdr: Sender<LiveParameters>, rcvr: Receiver<LiveParameters>, lp: LiveParameters, info: SndFileInfo) -> Self {
        Stream {
            pa_stream: pa_s,
            tx: sdr,
            rx: rcvr,
            state: lp,
            info: info,
            controller: None
        }
    }
    /// Start playing music.
    pub fn run(&mut self) {
        self.pa_stream.start().unwrap();
        while let true = self.pa_stream.is_active().unwrap() {
            if let Ok(params) = self.rx.try_recv() {
                self.state = params;
            }
            let sel = CDuration::seconds((self.state.frames_written / (self.info.samplerate as u64)) as i64);
            let stl = CDuration::seconds((self.state.frames_total / (self.info.samplerate as u64)) as i64);
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
                     lin_db(self.state.vol),
                     ca_str
            );
            if self.controller.is_some() {
                let params = self.controller.as_mut().unwrap().ctl(self.state);
                if params.is_none() {
                    self.controller = None;
                    println!("controller done");
                    continue;
                }
                self.tx.send(params.unwrap()).unwrap();
                self.state = params.unwrap();
                thread::sleep(self.controller.as_mut().unwrap().accuracy());
            }
            else {
                self.tx.send(self.state).unwrap();
                thread::sleep(Duration::from_millis(200));
            }
        }
        self.pa_stream.stop().unwrap();
    }
    /// Attach a `StreamController` to this stream.
    ///
    /// Currently just overwrites the existing one, if present.
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
    fn new(fade: (f32, f32), spec: u64, time: u64) -> Self {
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

/// Make a new `Stream` from a PortAudio instance and a sound file.
pub fn from_file<'a>(pa: &'a mut pa::PortAudio, mut file: SndFile) -> Result<Stream<'a>, pa::Error> {


    println!("PortAudio:");
    println!("version: {}", pa.version());
    println!("version text: {:?}", pa.version_text());
    println!("host count: {}", try!(pa.host_api_count()));

    let default_host = try!(pa.default_host_api());
    println!("default host: {:#?}", pa.host_api_info(default_host));

    let def_output = try!(pa.default_output_device());
    let output_info = try!(pa.device_info(def_output));
    println!("Default output device info: {:#?}", &output_info);

    // Construct the output stream parameters.
    let latency = output_info.default_low_output_latency;
    let output_params: pa::StreamParameters<f32> = pa::StreamParameters::new(def_output, file.info.channels, true, latency);

    // Check that the stream format is supported.
    try!(pa.is_output_format_supported(output_params, file.info.samplerate as f64));

    // Construct the settings with which we'll open our duplex stream.
    let settings = pa::stream::OutputSettings::new(output_params, file.info.samplerate as f64, SAMPLES_PER_CALLBACK);

    let mut live_params = LiveParameters {
        vol: 0.0,
        frames_written: 0,
        frames_total: file.info.frames as u64
    };

    let mut stream_cpy = live_params.clone();
    let (tx, rx) = channel();
    let (txs, rxs) = channel();
    let file_info = file.info.clone();
    // A callback to pass to the non-blocking stream.
    let callback = move |pa::stream::OutputCallbackArgs { buffer, frames, .. }| {
        let written = file.into_slice_float(buffer, frames).unwrap();
        let msg = rx.try_recv();
        let mut send: bool = false;
        if msg.is_ok() {
            let new_params: LiveParameters = msg.unwrap();
            stream_cpy.vol = new_params.vol;
            send = true;
        }
        for smpl in buffer.iter_mut() {
            *smpl = *smpl * stream_cpy.vol;
        }
        if written < frames {
            return pa::Complete;
        }
        stream_cpy.frames_written += written as u64;
        if send {
            txs.send(stream_cpy).unwrap();
        }
        //println!("{}/{} - current vol: {}dB", stream_cpy.frames_written, stream_cpy.frames_total, lin_db(stream_cpy.vol));
        if stream_cpy.frames_total >= stream_cpy.frames_written { pa::Continue } else { pa::Complete }
    };

    // Construct a stream with input and output sample types of f32.
    let mut stream = try!(pa.open_non_blocking_stream(settings, callback));
    let mut proper_stream = Stream::new(stream, tx, rxs, live_params, file_info);

    proper_stream.attach(Box::new(FadeController::new((-20.0, 0.0), 100, 5000)));
    Ok(proper_stream)
}
