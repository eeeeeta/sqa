#![feature(integer_atomics, test)]

extern crate rsoundio;
extern crate rsndfile;
extern crate bounded_spsc_queue;
extern crate time;
extern crate arrayvec;
extern crate test;

use std::sync::atomic::{AtomicBool, AtomicUsize, AtomicU64};
use std::sync::atomic::Ordering::*;
use bounded_spsc_queue::{Consumer, Producer};
use arrayvec::ArrayVec;
use std::sync::Arc;
use rsoundio::{SioFormat, SoundIo, OutStream, SioBackend, SioResult, Device};
use time::Duration;

const PLAYERS_PER_STREAM: usize = 256;
const STREAM_BUFFER_SIZE: usize = 50_000;
const ONE_SECOND_IN_NANOSECONDS: u64 = 1_000_000_000;

struct Sender {
    position: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    start_time: Arc<AtomicU64>,
    output_patch: Arc<AtomicUsize>,
    buf: Producer<f32>,
    sample_rate: u64
}
impl Sender {
    fn buf(&mut self) -> &mut Producer<f32> {
        &mut self.buf
    }
    fn set_active(&mut self, active: bool) {
        self.active.store(active, Relaxed);
    }
    fn active(&self) -> bool {
        self.active.load(Relaxed)
    }
    fn alive(&self) -> bool {
        self.alive.load(Relaxed)
    }
    fn position_samples(&self) -> u64 {
        self.position.load(Relaxed)
    }
    fn position(&self) -> Duration {
        Duration::nanoseconds((self.position.load(Relaxed) / (self.sample_rate * ONE_SECOND_IN_NANOSECONDS)) as i64)
    }
    fn output_patch(&self) -> usize {
        self.output_patch.load(Relaxed)
    }
    fn set_output_patch(&mut self, patch: usize) {
        self.output_patch.store(patch, Relaxed);
    }
}
impl Drop for Sender {
    fn drop(&mut self) {
        self.active.store(false, Relaxed);
        self.alive.store(false, Relaxed);
    }
}
struct Player {
    buf: Consumer<f32>,
    sample_rate: u64,
    start_time: Arc<AtomicU64>,
    position: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    output_patch: Arc<AtomicUsize>
}
impl Drop for Player {
    fn drop(&mut self) {
        self.active.store(false, Relaxed);
        self.alive.store(false, Relaxed);
    }
}
struct DeviceContext {
    players: ArrayVec<[Player; PLAYERS_PER_STREAM]>,
    control: Consumer<AudioThreadCommand>,
    length: Arc<AtomicUsize>,
    frames_per_cb: u32,
    sample_rate: u64
}
impl DeviceContext {
    #[inline(always)]
    fn handle(&mut self, cmd: AudioThreadCommand) {
        match cmd {
            AudioThreadCommand::AddPlayer(p) => {
                if self.players.push(p).is_none() {
                    let len = self.length.load(Acquire);
                    self.length.store(len + 1, Release);
                    self.players[self.players.len()-1].alive.store(true, Release);
                }
            }
        }
    }
    #[inline(always)]
    fn callback(&mut self, mut out: OutStream, min_frames: u32, max_frames: u32) -> SioResult<()> {
        let time = time::precise_time_ns();
        if let Some(cmd) = self.control.try_pop() {
            self.handle(cmd);
        }
        let frames_to_write = if min_frames == 0 {
            if max_frames > self.frames_per_cb {
                self.frames_per_cb
            }
            else {
                max_frames
            }
        } else {
            min_frames
        };
        let mut siter = out.write_stream_f32(frames_to_write as i32)?;
        let mut to_remove = None;
        'outer: for (idx, player) in self.players.iter_mut().rev().enumerate() {
            if !player.alive.load(Relaxed) {
                if to_remove.is_none() {
                    to_remove = Some(idx);
                }
                continue;
            }
            if !player.active.load(Relaxed) {
                continue;
            }
            let start_time = player.start_time.load(Relaxed);
            if start_time > time {
                player.position.store(0, Relaxed);
                continue;
            }
            let sample_delta = (time - start_time) / (self.sample_rate * ONE_SECOND_IN_NANOSECONDS);
            let mut pos = player.position.load(Relaxed);
            while pos+1 < sample_delta {
                if player.buf.try_pop().is_none() {
                    continue 'outer;
                }
                pos += 1;
            }
            if player.buf.size() < frames_to_write as usize {
                continue;
            }
            let outpatch = player.output_patch.load(Relaxed);
            if siter.channel(outpatch) {
                for x in &mut siter {
                    if let Some(data) = player.buf.try_pop() {
                        *x += data;
                        pos += 1;
                    }
                }
            }
            player.position.store(pos, Relaxed);
        }
        if let Some(x) = to_remove {
            self.players.swap_remove(x);
            self.length.store(self.length.load(Relaxed) - 1, Relaxed);
        }
        Ok(())
    }
}
struct EngineDevice {
    out: OutStream,
    dev: Device,
    length: Arc<AtomicUsize>,
    sample_rate: u64,
    control: Producer<AudioThreadCommand>
}

impl EngineDevice {
    fn num_senders(&self) -> usize {
        self.length.load(Relaxed)
    }
    fn new_sender(&mut self) -> Sender {
        let (p, c) = bounded_spsc_queue::make(STREAM_BUFFER_SIZE);
        let active = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(false));
        let position = Arc::new(AtomicU64::new(0));
        let start_time = Arc::new(AtomicU64::new(0));
        let output_patch = Arc::new(AtomicUsize::new(0));

        self.control.push(AudioThreadCommand::AddPlayer(Player {
            buf: c,
            sample_rate: self.sample_rate,
            start_time: start_time.clone(),
            position: position.clone(),
            active: active.clone(),
            alive: alive.clone(),
            output_patch: output_patch.clone()
        }));

        Sender {
            buf: p,
            position: position,
            active: active,
            alive: alive,
            output_patch: output_patch,
            start_time: start_time,
            sample_rate: self.sample_rate
        }
    }
}
struct EngineContext {
    sio: SoundIo,
    frames_per_cb: u32
}
impl EngineContext {
    fn new(frames_per_cb: u32) -> Self {
        EngineContext {
            sio: SoundIo::new("SQA Engine beta1"),
            frames_per_cb: frames_per_cb
        }
    }
    fn available_backends(&self) -> Vec<SioBackend> {
        let mut ret = vec![];
        for idx in 0..self.sio.backend_count() {
            if let Some(bk) = self.sio.backend(idx) {
                ret.push(bk);
            }
        }
        ret
    }
    fn available_devices(&self) -> (usize, Vec<Device>) {
        self.sio.flush_events();
        let mut ret = vec![];
        for idx in 0..self.sio.output_device_count().unwrap_or(0) {
            if let Some(bk) = self.sio.output_device(idx) {
                ret.push(bk);
            }
        }
        (self.sio.default_output_device_index().unwrap() as usize, ret)
    }
    fn connect_backend(&mut self, backend: SioBackend) -> SioResult<()> {
        self.sio.connect_backend(backend)
    }
    fn connect_auto(&mut self) -> SioResult<()> {
        self.sio.connect()
    }
    fn open_device(&mut self, dev: Device) -> SioResult<EngineDevice> {
        let sample_rate = dev.nearest_sample_rate(44_100);
        let mut out = dev.create_outstream()?;

        if cfg!(target_endian = "big") {
            out.set_format(SioFormat::Float32BE)?;
        }
        else {
            out.set_format(SioFormat::Float32LE)?;
        }
        out.set_sample_rate(sample_rate);
        out.set_name("SQA Engine beta1")?;
        let (p, c) = bounded_spsc_queue::make(128);
        let len1 = Arc::new(AtomicUsize::new(0));
        let len2 = len1.clone();
        out.register_write_callback(move |mut os: OutStream, min_frames: u32, max_frames: u32| {
            unsafe {
                if let Some(ctx) = os.unstash_data::<DeviceContext>() {
                    // FIXME: some sort of error atomic bool?
                    let _ = ctx.callback(os, min_frames, max_frames);
                }
            }
        });
        out.open()?;
        let dc = Box::new(DeviceContext {
            players: ArrayVec::new(),
            control: c,
            length: len1,
            frames_per_cb: self.frames_per_cb,
            sample_rate: out.sample_rate() as u64
        });
        out.stash_data(dc);
        out.set_latency(out.sample_rate() as f64 / self.frames_per_cb as f64);
        out.start()?;
        let sr = out.sample_rate() as u64;
        Ok(EngineDevice {
            out: out,
            sample_rate: sr,
            dev: dev,
            length: len2,
            control: p,
        })
    }
}
enum AudioThreadCommand {
    AddPlayer(Player)
}

const TABLE_SIZE: usize = 200;
use std::f32::consts::PI as PI32;
use std::thread;
fn main() {
    let mut ec = EngineContext::new(512);
    ec.connect_auto().unwrap();
    let (idx, devs) = ec.available_devices();
    let mut ed = ec.open_device(devs.into_iter().nth(idx).unwrap()).unwrap();
    let mut sender = ed.new_sender();
    thread::spawn(move || {
       const LEN: usize = 500 / 16;
        let mut pos = 0;
        loop {
            const F: f32 = 440.0;
            const W: f32 = 2.0 * F * PI32 / 48_000.0;
            const A: f32 = 0.6;
            const CYCLE: usize = (48_000f32 / F) as usize;

            let samples: Vec<f32> = (0..LEN)
                                        .map(|i| (W * (i + pos) as f32).sin() * A)
                .collect();
            for n in samples {
                sender.buf().push(n);
            }
            sender.set_active(true);
            pos = (pos + LEN) % CYCLE;
        }
    }).join().unwrap();
}
