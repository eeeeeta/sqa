use portaudio as pa;
use rsndfile::SndFile;
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;
use std::thread;

fn lin_db(lin: f32) -> f32 {
    lin.log10() * 20.0
}
fn db_lin(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}
#[derive(Debug, Clone, Copy)]
pub struct LiveParameters {
    pub vol: f32,
    pub frames_written: u64,
    pub frames_total: u64
}
pub trait StreamController {
    fn accuracy(&self) -> Duration;
    fn ctl(&mut self, last: LiveParameters) -> Option<LiveParameters>;
}
pub struct Stream<'a> {
    pa_stream: pa::stream::Stream<'a, pa::stream::NonBlocking, pa::stream::Output<f32>>,
    pub state: LiveParameters,
    tx: Sender<LiveParameters>,
    controller: Option<Box<StreamController>>
}
impl<'a> Stream<'a> {
    pub fn new(pa_s: pa::stream::Stream<'a, pa::stream::NonBlocking, pa::stream::Output<f32>>, sdr: Sender<LiveParameters>, lp: LiveParameters) -> Self {
        Stream {
            pa_stream: pa_s,
            tx: sdr,
            state: lp,
            controller: None
        }
    }
    pub fn run(&mut self) {
        self.pa_stream.start().unwrap();
        while let true = self.pa_stream.is_active().unwrap() {
            if self.controller.is_some() {
                let params = self.controller.as_mut().unwrap().ctl(self.state);
                if params.is_none() {
                    self.controller = None;
                    println!("controller done");
                    continue;
                }
                self.tx.send(params.unwrap()).unwrap();
                self.state = params.unwrap(); // XXX: should recv params from stream
                thread::sleep(self.controller.as_mut().unwrap().accuracy());
            }
        }
        self.pa_stream.stop().unwrap();
    }
    pub fn attach(&mut self, sc: Box<StreamController>) {
        self.controller = Some(sc);
    }
}

#[derive(Debug)]
struct FadeController {
    fade: (f32, f32),
    spec: u64,
    time: u64,
    fade_per_cnt: f32
}
impl FadeController {
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
        println!("{:?}", last);
        Some(last)
    }
}


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
    let settings = pa::stream::OutputSettings::new(output_params, file.info.samplerate as f64, 50);

    let mut live_params = LiveParameters {
        vol: 0.0,
        frames_written: 0,
        frames_total: file.info.frames as u64
    };

    let mut stream_cpy = live_params.clone();
    let (tx, rx) = channel();
    // A callback to pass to the non-blocking stream.
    let callback = move |pa::stream::OutputCallbackArgs { buffer, frames, .. }| {
        let written = file.into_slice_float(buffer, frames).unwrap();
        let msg = rx.try_recv();
        if msg.is_ok() {
            let new_params: LiveParameters = msg.unwrap();
            stream_cpy.vol = new_params.vol;
        }
        for smpl in buffer.iter_mut() {
            *smpl = *smpl * stream_cpy.vol;
        }
        if written < frames {
            return pa::Complete;
        }
        stream_cpy.frames_written += written as u64;
        println!("{}/{} - current vol: {}dB", stream_cpy.frames_written, stream_cpy.frames_total, lin_db(stream_cpy.vol));
        if stream_cpy.frames_total >= stream_cpy.frames_written { pa::Continue } else { pa::Complete }
    };

    // Construct a stream with input and output sample types of f32.
    let mut stream = try!(pa.open_non_blocking_stream(settings, callback));
    let mut proper_stream = Stream::new(stream, tx, live_params);

    proper_stream.attach(Box::new(FadeController::new((-20.0, 0.0), 100, 5000)));
    Ok(proper_stream)
}
