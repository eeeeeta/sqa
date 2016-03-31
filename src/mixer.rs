//! Sound mixing and device infrastructure.
use portaudio as pa;
use streamv2;
use std::time::Duration;
use std::ops::DerefMut;
use std::sync::{Arc, Mutex};

pub fn fill_with_silence(buf: &mut [f32]) {
    for smpl in buf.iter_mut() {
        *smpl = 0.0;
    }
}
pub trait Mixable {
    fn callback(&mut self, buffer: &mut [f32], frames: usize);
    fn sample_rate(&self) -> u64;
    fn frames_hint(&mut self, frames: usize);
}
pub struct RudimentaryMixer<'a> {
    pub stream: pa::stream::Stream<'a, pa::stream::NonBlocking, pa::stream::Output<f32>>,
    pub c1: Arc<Mutex<Option<Box<Mixable>>>>,
    pub c2: Arc<Mutex<Option<Box<Mixable>>>>,
    fpc: usize
}
impl<'a> RudimentaryMixer<'a> {
    pub fn new(mut pa: &'a mut pa::PortAudio, fpc: usize) -> Result<Self, pa::error::Error> {
        println!("PortAudio:");
        println!("version: {}", pa.version());
        println!("version text: {:?}", pa.version_text());
        println!("host count: {}", try!(pa.host_api_count()));

        let default_host = try!(pa.default_host_api());
        println!("default host: {:#?}", pa.host_api_info(default_host));
        let mut def_output = try!(pa.default_output_device());
        let output_info = try!(pa.device_info(def_output));
        println!("Default output device info: {:#?}", &output_info);
        let output_params: pa::StreamParameters<f32> = pa::StreamParameters::new(def_output, 2, true, output_info.default_low_output_latency);
        try!(pa.is_output_format_supported(output_params, 44_100.0_f64));
        let settings = pa::stream::OutputSettings::new(output_params, 44_100.0_f64, fpc as u32);
        let (mut buf1, mut buf2): (Vec<f32>, Vec<f32>) = (Vec::with_capacity(fpc), Vec::with_capacity(fpc));
        for _ in 0..fpc {
            buf1.push(0.0);
            buf2.push(0.0);
        }
        let (c1, c2) = (Arc::new(Mutex::new(None)), Arc::new(Mutex::new(None)));
        let (sc1, sc2) = (c1.clone(), c2.clone());

        let callback = move |pa::stream::OutputCallbackArgs { buffer, frames, .. }| {
            let (mut c1, mut c2): (::std::sync::MutexGuard<Option<Box<Mixable>>>, ::std::sync::MutexGuard<Option<Box<Mixable>>>) = (sc1.lock().unwrap(), sc2.lock().unwrap());
            if c1.deref_mut().is_some() {
                c1.deref_mut().as_mut().unwrap().callback(&mut buf1, frames);
            }
            else {
                for smpl in &mut buf1 {
                    *smpl = 0.0;
                }
            }
            if c2.deref_mut().is_some() {
                c2.deref_mut().as_mut().unwrap().callback(&mut buf2, frames);
            }
            else {
                for smpl in &mut buf2 {
                    *smpl = 0.0;
                }
            }
            for (i, (c1, c2)) in buf1.iter().zip(buf2.iter()).enumerate() {
                buffer[i*2] = *c1;
                buffer[(i*2)+1] = *c2;
            }
            pa::Continue
        };
        println!("Mixer created");
        let stream = try!(pa.open_non_blocking_stream(settings, callback));
        Ok(RudimentaryMixer {
            stream: stream,
            c1: c1,
            c2: c2,
            fpc: fpc
        })
    }
}
pub struct QChannel {
    clients: Arc<Mutex<Vec<Box<Mixable>>>>,
    c_buf: Vec<f32>,
    sample_rate: u64
}
impl QChannel {
    pub fn new(sample_rate: u64) -> Self {
        QChannel {
            clients: Arc::new(Mutex::new(Vec::new())),
            c_buf: Vec::new(),
            sample_rate: sample_rate
        }
    }
    pub fn add_client(&mut self,cli: Box<Mixable>) {
        self.clients.lock().unwrap().push(cli);
    }
}

impl Mixable for QChannel {
    fn callback(&mut self, buffer: &mut [f32], frames: usize) {
        let mut clients = self.clients.lock().unwrap();
        for (i, client) in clients.iter_mut().enumerate() {
            assert!(self.c_buf.len() == frames, "QChannel buf not big enough - did you remember to call frames_hint()?");
            client.callback(&mut self.c_buf, frames);
            for (out, inp) in buffer.iter_mut().zip(self.c_buf.iter()) {
                if i == 0 {
                    /* Zero the output buffer if it hasn't been
                    written to yet.
                    https://twitter.com/eeeeeta9/status/714118361203023873 */
                    *out = 0.0;
                }
                *out = *inp + *out;
                if *out > 1.0 {
                    *out = 1.0;
                }
                if *out < -1.0 {
                    *out = -1.0;
                }
            }
        }
    }
    fn sample_rate(&self) -> u64 {
        self.sample_rate
    }
    fn frames_hint(&mut self, frames: usize) {
        self.c_buf = Vec::with_capacity(frames);
        for _ in 0..frames {
            self.c_buf.push(0.0);
        }
    }
}
