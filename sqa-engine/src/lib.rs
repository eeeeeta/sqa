//! The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL
//! NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED",  "MAY", and
//! "OPTIONAL" in this document are to be interpreted as described in
//! [RFC 2119](https://tools.ietf.org/html/rfc2119).
//!
//! The key words "MUST (BUT WE KNOW YOU WON'T)", "SHOULD CONSIDER",
//! "REALLY SHOULD NOT", "OUGHT TO", "WOULD PROBABLY", "MAY WISH TO",
//! "COULD", "POSSIBLE", and "MIGHT" in this document are to be
//! interpreted as described in [RFC 6919](https://tools.ietf.org/html/rfc6919).
#![feature(integer_atomics)]

pub extern crate sqa_jack;
extern crate bounded_spsc_queue;
extern crate time;
extern crate arrayvec;
#[macro_use]
extern crate error_chain;
extern crate parking_lot;
extern crate uuid;

pub mod errors;
pub mod sync;
mod thread;

use std::sync::atomic::{AtomicBool, AtomicUsize, AtomicU64, AtomicU32};
use std::sync::atomic::Ordering::*;
use bounded_spsc_queue::Producer;
use arrayvec::ArrayVec;
use std::sync::Arc;
use std::mem;
use time::Duration;
use sqa_jack::*;
pub use errors::EngineResult;
use errors::{ErrorKind};
pub use uuid::Uuid;
pub use sqa_jack as jack;
/// The maximum amount of streams that can play concurrently.
///
/// Can be increased to 512 with the `512-players` feature.
#[cfg(not(feature = "512-players"))]
pub const MAX_PLAYERS: usize = 256;
#[cfg(feature = "512-players")]
pub const MAX_PLAYERS: usize = 512;
/// The maximum amount of channels that can be created.
///
/// Can be increased to 128 with the `128-channels` feature.
#[cfg(not(feature = "128-channels"))]
pub const MAX_CHANS: usize = 64;
#[cfg(feature = "128-channels")]
pub const MAX_CHANS: usize = 128;
/// The size of a stream's buffer, in samples.
pub const STREAM_BUFFER_SIZE: usize = 100_000;
/// The size of the communication buffer between audio thread and main thread, in messages.
pub const CONTROL_BUFFER_SIZE: usize = MAX_PLAYERS * 2;
/// One second, in nanoseconds.
const ONE_SECOND_IN_NANOSECONDS: u64 = 1_000_000_000;

/// Corresponds to, and controls, a `Player` in the audio thread.
pub struct Sender<T> {
    /// Current position, in samples from the start of the buffer (read only)
    position: Arc<AtomicU64>,
    /// Whether this stream will play samples (rw)
    active: Arc<AtomicBool>,
    /// Whether this stream is dead (rw)
    alive: Arc<AtomicBool>,
    /// When (from the system's monotonic clock) the player should begin playback (rw)
    start_time: Arc<AtomicU64>,
    /// Which channel number this stream is patched to (rw)
    output_patch: Arc<AtomicUsize>,
    /// The playback volume (actually a f32 transmuted!) (rw)
    volume: Arc<AtomicU32>,
    /// The buffer to write to (or not) - will be a `bounded_spsc_queue::Producer<f32>` or `()`.
    pub buf: T,
    /// The sample rate of this sender. Can differ from the output sample rate.
    pub sample_rate: u64,
    /// Whether this sender was the original, or a clone.
    original: bool,
    /// The UUID of this sender.
    uuid: Uuid
}
/// A `Sender` which can write data to its `Player`'s buffer.
pub type BufferSender = Sender<Producer<f32>>;
/// A `Sender` which cannot write data to its `Player`'s buffer.
pub type PlainSender = Sender<()>;
impl<T> Sender<T> {
    /// Set whether this stream will play samples or not.
    ///
    /// This essentially halts all processing related to the sender's `Player`.
    pub fn set_active(&mut self, active: bool) {
        self.active.store(active, Relaxed);
    }
    /// Start playing the stream, from this moment on.
    ///
    /// This calls `set_start_time()` with the current time, and calls `set_active(true)`.
    pub fn unpause(&mut self) {
        self.set_start_time(time::precise_time_ns());
        self.set_active(true);
    }
    /// Start playing the stream, as if it was supposed to start at a given time.
    ///
    /// This calls `set_start_time()` with the given time, and calls `set_active(true)`.
    pub fn play_from_time(&mut self, time: u64) {
        self.set_start_time(time);
        self.set_active(true);
    }
    /// Set the volume of this stream.
    ///
    /// This volume is linear: a value of `1.0` means 0dB.
    pub fn set_volume(&mut self, vol: f32) {
        let val = unsafe {
            mem::transmute::<f32, u32>(vol)
        };
        self.volume.store(val, Relaxed);
    }
    /// Get the volume of this stream.
    ///
    /// This volume is linear: a value of `1.0` means 0dB.
    pub fn volume(&self) -> f32 {
        let val = self.volume.load(Relaxed);
        unsafe {
            mem::transmute::<u32, f32>(val)
        }
    }
    /// Get whether this stream will play samples or not.
    pub fn active(&self) -> bool {
        self.active.load(Relaxed)
    }
    /// Query whether this stream is alive. If this function returns `false`, any other action on this stream has no effect - the stream
    /// is no longer being processed.
    pub fn alive(&self) -> bool {
        self.alive.load(Relaxed)
    }
    /// Resets this stream's position to 0.
    ///
    /// This will also reset its `start_time` to the current time as a preventative measure against calling this function without doing so
    /// while the stream is playing.
    pub fn reset_position(&mut self) {
        self.set_start_time(time::precise_time_ns());
        self.position.store(0, Relaxed);
    }
    /// Get the stream's position in samples.
    ///
    /// This position starts at 0 when the stream starts, and is incremented every time the stream delivers samples.
    /// It is compared to the `start_time`, meaning that you MUST NOT change one without changing the other (otherwise, the stream will
    /// think it's out of sync). In fact, you can't!
    pub fn position_samples(&self) -> u64 {
        self.position.load(Relaxed)
    }
    /// Get the stream's position as a `Duration`.
    pub fn position(&self) -> Duration {
        Duration::milliseconds(((self.position.load(Relaxed) as f64 / self.sample_rate as f64) * 1000.0)as i64)
    }
    /// Get this stream's output patch (which channel number this stream is patched to)
    pub fn output_patch(&self) -> usize {
        self.output_patch.load(Relaxed)
    }
    /// Set this stream's output patch (which channel number this stream is patched to)
    ///
    /// An invalid output patch will cause the stream to deactivate (`active` will be set to false).
    pub fn set_output_patch(&mut self, patch: usize) {
        self.output_patch.store(patch, Relaxed);
    }
    /// Set this stream's start time - the time, from the system's monotonic clock, that it starts playing at.
    ///
    /// The stream will maintain its playback position relative to this start time, skipping frames as needed to catch up.
    /// To get the current time from the system's monotonic clock, call `Sender::precise_time_ns`.
    pub fn set_start_time(&mut self, st: u64) {
        self.start_time.store(st, Relaxed);
    }
    /// Make a `PlainSender` from this sender.
    pub fn make_plain(&self) -> PlainSender {
        Sender {
            position: self.position.clone(),
            active: self.active.clone(),
            alive: self.alive.clone(),
            start_time: self.start_time.clone(),
            output_patch: self.output_patch.clone(),
            volume: self.volume.clone(),
            buf: (),
            sample_rate: self.sample_rate,
            original: false,
            uuid: self.uuid
        }
    }
    /// Get this sender's UUID.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }
    /// A wrapper around `time::precise_time_ns()`.
    #[inline(always)]
    pub fn precise_time_ns() -> u64 {
        time::precise_time_ns()
    }
}
impl<T> Drop for Sender<T> {
    /// If this sender was the original: deactivates the stream, setting `alive` to false.
    fn drop(&mut self) {
        if self.original {
            self.active.store(false, Relaxed);
            self.alive.store(false, Relaxed);
        }
    }
}
/// Main engine context, containing a connection to JACK.
pub struct EngineContext {
    pub conn: JackConnection<Activated>,
    pub chans: ArrayVec<[JackPort; MAX_CHANS]>,
    length: Arc<AtomicUsize>,
    control: Producer<thread::AudioThreadCommand>,
    rx: Option<sync::AudioThreadHandle>
}
impl EngineContext {
    /// Initialise the SQA Engine, opening a connection to JACK and starting the audio thread.
    ///
    /// The connection is made under a given name if provided, otherwise under "SQA Engine".
    pub fn new(name: Option<&str>) -> EngineResult<Self> {
        let len = Arc::new(AtomicUsize::new(0));
        let (p, c) = bounded_spsc_queue::make(CONTROL_BUFFER_SIZE);
        let (rc, rp) = unsafe { sync::AudioThreadHandle::make() };
        let mut conn = JackConnection::connect(name.unwrap_or("SQA Engine"), Some(OPEN_NO_START_SERVER))?;
        let dctx = thread::DeviceContext {
            players: ArrayVec::new(),
            chans: ArrayVec::new(),
            control: c,
            length: len.clone(),
            sample_rate: conn.sample_rate() as u64,
            sender: rp
        };
        conn.set_handler(dctx)?;
        let conn = match conn.activate() {
            Ok(c) => c,
            Err((_, err)) => return Err(err.into())
        };
        Ok(EngineContext {
            conn: conn,
            chans: ArrayVec::new(),
            length: len,
            control: p,
            rx: Some(rc)
        })
    }
    /// Obtain a communication channel to receive messages from the audio thread.
    /// Can only be called once - will return None after the first call.
    ///
    /// # Safety
    ///
    /// **WARNING:** In order to not leak memory, you MUST continually `recv()` from this handle
    /// to avoid filling the message queue. If the message queue is filled, the audio thread will
    /// leak any `Player`s that are removed or rejected, as it will not be able to send them through
    /// the channel (and deallocation would block the audio thread). (BUT WE KNOW YOU WON'T, because
    /// it requires spawning another thread)
    pub fn get_handle(&mut self) -> Option<sync::AudioThreadHandle> {
        self.rx.take()
    }
    pub fn num_senders(&self) -> usize {
        self.length.load(Relaxed)
    }
    pub fn new_channel(&mut self, name: &str) -> EngineResult<usize> {
        let port = self.conn.register_port(name, PORT_IS_OUTPUT | PORT_IS_TERMINAL)?;
        if self.chans.len() == self.chans.capacity() {
            Err(ErrorKind::LimitExceeded)?
        }
        self.control.push(thread::AudioThreadCommand::AddChannel(port.clone()));
        self.chans.push(port);
        Ok(self.chans.len()-1)
    }
    pub fn new_sender(&mut self, sample_rate: u64) -> BufferSender {
        let (p, c) = bounded_spsc_queue::make(STREAM_BUFFER_SIZE);
        let active = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(false));
        let position = Arc::new(AtomicU64::new(0));
        let start_time = Arc::new(AtomicU64::new(0));
        let one_f32_in_u32 = unsafe {
            mem::transmute::<f32, u32>(1.0)
        };
        let volume = Arc::new(AtomicU32::new(one_f32_in_u32));
        let output_patch = Arc::new(AtomicUsize::new(MAX_CHANS));
        let uu = Uuid::new_v4();

        self.control.push(thread::AudioThreadCommand::AddPlayer(thread::Player {
            buf: c,
            sample_rate: sample_rate,
            start_time: start_time.clone(),
            position: position.clone(),
            active: active.clone(),
            alive: alive.clone(),
            output_patch: output_patch.clone(),
            volume: volume.clone(),
            uuid: uu
        }));

        Sender {
            buf: p,
            position: position,
            active: active,
            alive: alive,
            output_patch: output_patch,
            start_time: start_time,
            sample_rate: sample_rate,
            volume: volume.clone(),
            original: true,
            uuid: uu
        }
    }
}
