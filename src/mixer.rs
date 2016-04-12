//! Sound mixing and device infrastructure.
use portaudio as pa;
use uuid::Uuid;
use std::ops::DerefMut;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::collections::BTreeMap;
pub const FRAMES_PER_CALLBACK: usize = 500;

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
pub struct Magister<'a> {
    /// Map of sink UUIDs to sinks.
    sinks: BTreeMap<Uuid, Box<Sink + 'a>>,
    /// Map of source UUIDs to sources.
    sources: BTreeMap<Uuid, Box<Source>>
}

impl<'a> Magister<'a> {
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
    pub fn add_sink(&mut self, sink: Box<Sink + 'a>) {
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

pub struct DeviceSink<'a> {
    pub stream: Rc<RefCell<pa::stream::Stream<'a, pa::stream::NonBlocking, pa::stream::Output<f32>>>>,
    chans: Arc<Mutex<Vec<Option<Box<Source>>>>>,
    id: usize,
    shared_uuid: Uuid,
    uuid: Uuid
}
impl<'a> Sink for DeviceSink<'a> {
    fn wire(&mut self, cli: Box<Source>) -> Option<Box<Source>> {
        let mut ret = None;
        {
            let ref mut this = self.chans.lock().unwrap()[self.id];
            if this.is_some() {
                ret = Some(this.take().unwrap());
            }
            *this = Some(cli);
        }
        self.start_stop_ck();
        ret
    }
    fn unwire(&mut self, uuid: Uuid) -> Option<Box<Source>> {
        let ret: Option<Box<Source>>;
        {
            let ref mut this = self.chans.lock().unwrap()[self.id];
            if this.is_some() && this.as_ref().unwrap().uuid() == uuid {
                ret = Some(this.take().unwrap());
            }
            else {
                ret = None;
            }
        }
        if ret.is_some() {
            self.start_stop_ck();
        }
        ret
    }
    fn uuid(&self) -> Uuid {
        self.uuid.clone()
    }
}
impl<'a> DeviceSink<'a> {
    fn start_stop_ck(&mut self) {
        /*
        println!("SS cking");
        let chans = self.chans.lock().unwrap();
        let mut stream = self.stream.borrow_mut();
        let mut start = false;
        for chan in chans.iter() {
            if chan.is_some() {
                start = true;
                break;
            }
        }
        if start && stream.is_stopped().unwrap() {
            println!("Starting");
            stream.start().unwrap();
        }
        else if stream.is_active().unwrap() {
            println!("Stopping");
            stream.stop().unwrap();
        }
        */
    }
    pub fn from_device_chans(pa: &'a mut pa::PortAudio, dev: pa::DeviceIndex) -> Result<Vec<Self>, pa::error::Error> {
        println!("PortAudio:");
        println!("version: {}", pa.version());
        println!("version text: {:?}", pa.version_text());
        println!("host count: {}", try!(pa.host_api_count()));

        let dev_info = try!(pa.device_info(dev));
        println!("Output device info: {:#?}", &dev_info);
        let params: pa::StreamParameters<f32> = pa::StreamParameters::new(dev, dev_info.max_output_channels, true, dev_info.default_low_output_latency);
        try!(pa.is_output_format_supported(params, 44_100.0_f64));
        let settings = pa::stream::OutputSettings::new(params, 44_100.0_f64, FRAMES_PER_CALLBACK as u32);

        let mut chans: Arc<Mutex<Vec<Option<Box<Source>>>>> = Arc::new(Mutex::new(Vec::new()));
        let mut chans_cb = chans.clone();
        let mut bufs: Vec<Vec<f32>> = Vec::new();
        let mut chans_lk = chans.lock().unwrap();
        for _ in 0..dev_info.max_output_channels {
            chans_lk.push(None);
            let mut buf: Vec<f32> = Vec::new();
            for _ in 0..FRAMES_PER_CALLBACK {
                buf.push(0.0);
            }
            bufs.push(buf);
        }
        let callback = move |pa::stream::OutputCallbackArgs { buffer, frames, .. }| {
            assert!(frames <= FRAMES_PER_CALLBACK as usize, "PA demanded more frames/cb than we asked for");
            for (i, ch) in chans_cb.lock().unwrap().iter_mut().enumerate() {
                if ch.is_some() {
                    ch.as_mut().unwrap().callback(&mut bufs[i], frames);
                }
            }
            let num_chans = bufs.len();
            for frame in 0..frames {
                for (chan, cbuf) in bufs.iter_mut().enumerate() {
                    buffer[(frame * num_chans) + chan] = cbuf[frame];
                    cbuf[frame] = 0.0;
                }
            }
            pa::Continue
        };
        let stream = Rc::new(RefCell::new(try!(pa.open_non_blocking_stream(settings, callback))));
        {
            stream.borrow_mut().start().unwrap();
        }
        let uuid = Uuid::new_v4();
        let mut rets: Vec<Self> = Vec::new();
        for id in 0..chans_lk.len() {
            rets.push(DeviceSink {
                stream: stream.clone(),
                chans: chans.clone(),
                id: id,
                shared_uuid: uuid.clone(),
                uuid: Uuid::new_v4()
            })
        }
        Ok(rets)
    }
}

pub struct QChannelX {
    clients: Arc<Mutex<Vec<Box<Source>>>>,
    uuid: Uuid,
    uuid_pair: Uuid
}
impl QChannelX {
    pub fn uuid_pair(&self) -> Uuid {
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
