//! Sound mixing and device infrastructure.
use portaudio as pa;
use uuid::Uuid;
use std::ops::DerefMut;
use std::sync::{Arc, Mutex};
use std::collections::BTreeMap;
/// Fills a given buffer with silence.
pub fn fill_with_silence(buf: &mut [f32]) {
    for smpl in buf.iter_mut() {
        *smpl = 0.0;
    }
}
/// Describes objects that can accept audio data from `Source`s.
pub trait Sink {
    /// Wires a given client to this sink.
    ///
    /// If this sink can only hold one client and it is full already,
    /// returns that client.
    fn wire(&mut self, cli: Box<Source>) -> Option<Box<Source>>;
    /// Retrieves the client with a given `uuid` from this sink.
    fn unwire(&mut self, uuid: Uuid) -> Option<Box<Source>>;
    /// Get this object's Universally Unique Identifier (UUID).
    fn uuid(&self) -> Uuid;
}

/// Describes objects that can provide a stream of audio data.
pub trait Source {
    /// Get more audio data from this object.
    ///
    /// As this is often called in a low-latency audio thread, it must try its best
    /// to not block and be efficient.
    fn callback(&mut self, buffer: &mut [f32], frames: usize);
    /// Get this object's sample rate.
    fn sample_rate(&self) -> u64;
    /// Give this object an idea of the amount of frames it will be expected to provide.
    ///
    /// This lets the object have time to allocate buffers it may need to perform mixing
    /// without blocking/allocating more.
    ///
    /// Proper mixer implementations will call this at instantiation, before calling
    /// any callbacks in a low-latency audio thread.
    /// It is considered acceptable for an implementation to panic if this invariant is violated.
    fn frames_hint(&mut self, frames: usize);
    /// Get this object's Universally Unique Identifier (UUID).
    fn uuid(&self) -> Uuid;
}
/// Details return values from a call to `wire()`.
#[derive(Debug)]
pub enum WireResult {
    /// Source could not be found (standalone, or wired to a known sink).
    SourceNotAvailable,
    /// Sink could not be found (not in `sinks` vec?).
    SinkNotAvailable,
    /// Successful, but had to displace a source of given UUID.
    ///
    /// (The source is placed into the `sources` of the `Magister`).
    DisplacedUuid(Uuid),
    /// Successful, nothing cool happened.
    Uneventful
}

/// Master of all things mixer-y.
///
/// Contains various `BTreeMap`s of everything related to mixing.
///
/// *magister, magistri (2nd declension masculine): master, teacher*
pub struct Magister {
    /// Map of sink UUIDs to sinks.
    sinks: BTreeMap<Uuid, Box<Sink>>,
    /// Map of source UUIDs to sources.
    sources: BTreeMap<Uuid, Box<Source>>
}

impl Magister {
    pub fn new() -> Self {
        let ms = Magister {
            sinks: BTreeMap::new(),
            sources: BTreeMap::new()
        };
        ms
    }
    pub fn add_source(&mut self, source: Box<Source>) {
        if let Some(_) = self.sources.insert(source.uuid(), source) {
            panic!("UUID collision")
        }
    }
    pub fn add_sink(&mut self, sink: Box<Sink>) {
        if let Some(_) = self.sinks.insert(sink.uuid(), sink) {
            panic!("UUID collision")
        }
    }
    pub fn wire(&mut self, from: Uuid, to: Uuid) -> Result<WireResult, WireResult> {
        let mut source: Option<Box<Source>> = self.sources.remove(&from);
        if source.is_none() {
            for (_, val) in self.sinks.iter_mut() {
                if let Some(src) = val.unwire(from) {
                    source = Some(src);
                    break;
                }
            }
        }
        if source.is_none() {
            return Err(WireResult::SourceNotAvailable)
        }
        let result: Option<Box<Source>>;
        {
            let sink: Option<&mut Box<Sink>> = self.sinks.get_mut(&to);
            if sink.is_none() {
                return Err(WireResult::SinkNotAvailable)
            }
            result = sink.unwrap().wire(source.unwrap());
        }
        if let Some(displaced) = result {
            let uuid = displaced.uuid();
            self.add_source(displaced);
            Ok(WireResult::DisplacedUuid(uuid))
        }
        else {
            Ok(WireResult::Uneventful)
        }
    }
}

/// Rudimentary, two-channel mixer.
///
/// Will be replaced with a better, multi-channel mixer that can choose devices properly.
pub struct RudimentaryMixer<'a> {
    /// Underlying PortAudio stream.
    pub stream: pa::stream::Stream<'a, pa::stream::NonBlocking, pa::stream::Output<f32>>,
    /// Channel 1.
    pub c1: Arc<Mutex<Option<Box<Source>>>>,
    /// Channel 2.
    pub c2: Arc<Mutex<Option<Box<Source>>>>,
    /// Frames per callback.
    fpc: usize
}

impl<'a> RudimentaryMixer<'a> {
    pub fn new(pa: &'a mut pa::PortAudio, fpc: usize) -> Result<Self, pa::error::Error> {
        println!("PortAudio:");
        println!("version: {}", pa.version());
        println!("version text: {:?}", pa.version_text());
        println!("host count: {}", try!(pa.host_api_count()));

        let default_host = try!(pa.default_host_api());
        println!("default host: {:#?}", pa.host_api_info(default_host));
        let def_output = try!(pa.default_output_device());
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
            let (mut c1, mut c2): (::std::sync::MutexGuard<Option<Box<Source>>>, ::std::sync::MutexGuard<Option<Box<Source>>>) = (sc1.lock().unwrap(), sc2.lock().unwrap());
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
pub struct QChannelX {
    clients: Arc<Mutex<Vec<Box<Source>>>>,
    uuid: Uuid,
    uuid_pair: Uuid
}
impl QChannelX {
    fn uuid_pair(&self) -> Uuid {
        self.uuid_pair.clone()
    }
}
impl Sink for QChannelX {
    fn uuid(&self) -> Uuid {
        self.uuid.clone()
    }
    fn wire(&mut self, cli: Box<Source>) -> Option<Box<Source>> {
        self.clients.lock().unwrap().push(cli);
        None
    }
    fn unwire(&mut self, uuid: Uuid) -> Option<Box<Source>> {
        let mut clients = self.clients.lock().unwrap();
        let mut client_idx: Option<usize> = None;
        for (i, cli) in clients.iter().enumerate() {
            if cli.uuid() == uuid {
                client_idx = Some(i);
                break;
            }
        }
        if client_idx.is_none() {
            None
        }
        else {
            Some(clients.remove(client_idx.unwrap()))
        }
    }
}
pub struct QChannel {
    clients: Arc<Mutex<Vec<Box<Source>>>>,
    c_buf: Vec<f32>,
    sample_rate: u64,
    uuid: Uuid
}
impl QChannel {
    pub fn new(sample_rate: u64) -> Self {
        QChannel {
            clients: Arc::new(Mutex::new(Vec::new())),
            c_buf: Vec::new(),
            sample_rate: sample_rate,
            uuid: Uuid::new_v4()
        }
    }
    pub fn get_x(&self) -> QChannelX {
        QChannelX {
            clients: self.clients.clone(),
            uuid: Uuid::new_v4(),
            uuid_pair: self.uuid.clone()
        }
    }
}

impl Source for QChannel {
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
    fn uuid(&self) -> Uuid {
        self.uuid.clone()
    }
}
