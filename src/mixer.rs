//! Sound mixing and device infrastructure.
use portaudio as pa;
use streamv2;
use std::time::Duration;
use std::ops::DerefMut;
pub trait Mixable {
    fn callback(&mut self, buffer: &mut [f32], frames: usize) -> CallbackResult;
    fn control(&mut self, time: Duration) -> ControlResult;
    fn sample_rate(&self) -> u64;
    fn frames_hint(&mut self, frames: usize);
}
pub enum ControlResult {
    Done,
    Useless
}
pub enum CallbackResult {
    More,
    None
}
pub struct RudimentaryMixer<'a> {
    pub stream: pa::stream::Stream<'a, pa::stream::NonBlocking, pa::stream::Output<f32>>
}
impl<'a> RudimentaryMixer<'a> {
    pub fn new(mut pa: &'a mut pa::PortAudio, mut c1: Box<Mixable>, mut c2: Box<Mixable>) -> Result<Self, pa::error::Error> {
        if c1.sample_rate() != 44_100 || c2.sample_rate() != 44_100 {
            panic!("RM sample rate mismatch");
        }
        println!("PortAudio:");
        println!("version: {}", pa.version());
        println!("version text: {:?}", pa.version_text());
        println!("host count: {}", try!(pa.host_api_count()));

        let default_host = try!(pa.default_host_api());
        println!("default host: {:#?}", pa.host_api_info(default_host));
        let mut def_output = try!(pa.default_output_device());
        for device in try!(pa.devices()) {
            let (idx, info) = try!(device);
            println!("--------------------------------------- {:?}", idx);
            println!("{:#?}", &info);
            if info.name.contains("pulse") {
                def_output = idx;
                println!("WE FOUND HIM");
                break;
            }

        }
        let output_info = try!(pa.device_info(def_output));
    println!("Default output device info: {:#?}", &output_info);
        let output_params: pa::StreamParameters<f32> = pa::StreamParameters::new(def_output, 2, true, output_info.default_low_output_latency);
        try!(pa.is_output_format_supported(output_params, 44_100.0_f64));
        let settings = pa::stream::OutputSettings::new(output_params, 44_100.0_f64, 500);
        let (mut buf1, mut buf2): (Vec<f32>, Vec<f32>) = (Vec::new(), Vec::new());
        for _ in 0..500 {
            buf1.push(0.0);
            buf2.push(0.0);
        }
        c1.deref_mut().frames_hint(500);
        c2.deref_mut().frames_hint(500);
        let callback = move |pa::stream::OutputCallbackArgs { buffer, frames, .. }| {
            let a = c1.deref_mut().callback(&mut buf1, 500);
            let b = c2.deref_mut().callback(&mut buf2, 500);
            if let CallbackResult::None = a {
                return pa::Complete;
            }
            if let CallbackResult::None = b {
                return pa::Complete;
            }
            for (i, (c1, c2)) in buf1.iter().zip(buf2.iter()).enumerate() {
                buffer[i*2] = *c1;
                buffer[(i*2)+1] = *c2;
            }
            pa::Continue
        };
        println!("Mixer created");
        Ok(RudimentaryMixer {
            stream: try!(pa.open_non_blocking_stream(settings, callback))
        })
    }
}

pub struct QChannel<'a> {
    clients: Vec<Box<Mixable + 'a>>,
    c_buf: Vec<f32>,
    sample_rate: u64
}
impl<'a> QChannel<'a> {
    pub fn new(sample_rate: u64) -> Self {
        QChannel {
            clients: Vec::new(),
            c_buf: Vec::new(),
            sample_rate: sample_rate
        }
    }
    pub fn add_client(&mut self, cli: Box<Mixable + 'a>) {
        if cli.sample_rate() != self.sample_rate {
            panic!("QChannel sample rate mismatch");
        }
        self.clients.push(cli);
    }
}

impl<'a> Mixable for QChannel<'a> {
    fn callback(&mut self, buffer: &mut [f32], frames: usize) -> CallbackResult {
        let mut to_rem: Vec<usize> = Vec::with_capacity(self.clients.len());
        for (i, client) in self.clients.iter_mut().enumerate() {
            assert!(self.c_buf.len() == frames);
            if let CallbackResult::None = client.callback(&mut self.c_buf, frames) {
                to_rem.push(i);
            }
            else {
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
        if to_rem.len() == self.clients.len() {
            CallbackResult::None
        }
        else {
            to_rem.sort();
            let mut iter = to_rem.into_iter();
            while let Some(idx) = iter.next_back() {
                self.clients.remove(idx);
            }
            CallbackResult::More
        }
    }
    fn control(&mut self, time: Duration) -> ControlResult {
        // FIXME FIXME FIXME
        ControlResult::Useless
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
