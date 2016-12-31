#![feature(integer_atomics, test)]

extern crate sqa_jack;
extern crate bounded_spsc_queue;
extern crate time;
extern crate arrayvec;
extern crate hound;
extern crate test;

use std::sync::atomic::{AtomicBool, AtomicUsize, AtomicU64};
use std::sync::atomic::Ordering::*;
use bounded_spsc_queue::{Consumer, Producer};
use arrayvec::ArrayVec;
use std::sync::Arc;
use time::Duration;
use sqa_jack::*;

const MAX_PLAYERS: usize = 256;
const MAX_CHANS: usize = 64;
const STREAM_BUFFER_SIZE: usize = 50_000;
const ONE_SECOND_IN_NANOSECONDS: u64 = 1_000_000_000;

struct Sender<T> {
    position: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    start_time: Arc<AtomicU64>,
    output_patch: Arc<AtomicUsize>,
    sync: Arc<AtomicBool>,
    buf: T,
    sample_rate: u64,
    original: bool
}
type UsefulSender = Sender<Producer<f32>>;
type PlainSender = Sender<()>;
impl<T> Sender<T> {
    fn buf(&mut self) -> &mut T {
        &mut self.buf
    }
    fn set_active(&mut self, active: bool) {
        self.active.store(active, Relaxed);
    }
    fn set_sync(&mut self, sync: bool) {
        self.sync.store(sync, Relaxed);
    }
    fn sync(&self) -> bool {
        self.sync.load(Relaxed)
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
        Duration::milliseconds(((self.position.load(Relaxed) as f64 / self.sample_rate as f64) * 1000.0)as i64)
    }
    fn output_patch(&self) -> usize {
        self.output_patch.load(Relaxed)
    }
    fn set_output_patch(&mut self, patch: usize) {
        self.output_patch.store(patch, Relaxed);
    }
    fn set_start_time(&mut self, st: u64) {
        self.start_time.store(st, Relaxed);
    }
    fn make_plain(&self) -> PlainSender {
        Sender {
            position: self.position.clone(),
            active: self.active.clone(),
            alive: self.alive.clone(),
            start_time: self.start_time.clone(),
            output_patch: self.output_patch.clone(),
            sync: self.sync.clone(),
            buf: (),
            sample_rate: self.sample_rate,
            original: false
        }
    }
}
impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if self.original {
        self.active.store(false, Relaxed);
            self.alive.store(false, Relaxed);
        }
    }
}
struct Player {
    buf: Consumer<f32>,
    sample_rate: u64,
    start_time: Arc<AtomicU64>,
    position: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    output_patch: Arc<AtomicUsize>,
    sync: Arc<AtomicBool>
}
impl Drop for Player {
    fn drop(&mut self) {
        self.active.store(false, Relaxed);
        self.alive.store(false, Relaxed);
    }
}
struct DeviceChannel {
    port: JackPort,
    written_t: u64
}
struct DeviceContext {
    players: ArrayVec<[Player; MAX_PLAYERS]>,
    chans: ArrayVec<[DeviceChannel; MAX_CHANS]>,
    control: Consumer<AudioThreadCommand>,
    length: Arc<AtomicUsize>,
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
            },
            AudioThreadCommand::AddChannel(p) => {
                self.chans.push(DeviceChannel { port: p, written_t: 0 });
            }
        }
    }
}
impl JackHandler for DeviceContext {
    #[inline(always)]
    fn process(&mut self, out: &JackCallbackContext) -> JackControl {
        let time = time::precise_time_ns();
        if let Some(cmd) = self.control.try_pop() {
            self.handle(cmd);
        }
        let mut to_remove = None;
        'outer: for (idx, player) in self.players.iter_mut().enumerate() {
            if !player.alive.load(Relaxed) {
                if to_remove.is_none() {
                    to_remove = Some(idx);
                }
                continue;
            }
            if !player.active.load(Relaxed) {
                continue;
            }
            let outpatch = player.output_patch.load(Relaxed);
            if outpatch >= self.chans.len() {
                player.active.store(false, Relaxed);
                continue;
            }
            let start_time = player.start_time.load(Relaxed);
            if start_time > time {
                player.position.store(0, Relaxed);
                continue;
            }
            let sample_delta = (time - start_time) * self.sample_rate / ONE_SECOND_IN_NANOSECONDS;
            let mut pos = player.position.load(Relaxed);
            if pos < sample_delta {
                pos += player.buf.skip_n((sample_delta - pos) as usize) as u64;
            }
            if pos < sample_delta || player.buf.size() < out.nframes() as usize {
                player.position.store(pos, Relaxed);
                continue;
            }
            if let Some(buf) = out.get_port_buffer(&self.chans[outpatch].port) {
                let written = time == self.chans[outpatch].written_t;
                if !written {
                    self.chans[outpatch].written_t = time;
                }
                for x in buf.iter_mut() {
                    if let Some(data) = player.buf.try_pop() {
                        if written {
                            *x += data;
                        }
                        else {
                            *x = data;
                        }
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
        for ch in self.chans.iter_mut() {
            if ch.written_t != time {
                if let Some(buf) = out.get_port_buffer(&ch.port) {
                    for x in buf.iter_mut() {
                        *x = 0.0;
                    }
                }
            }
        }
        JackControl::Continue
    }
}
struct EngineContext {
    pub conn: JackConnection<Activated>,
    pub chans: ArrayVec<[JackPort; MAX_CHANS]>,
    length: Arc<AtomicUsize>,
    control: Producer<AudioThreadCommand>,
}
impl EngineContext {
    fn new(name: &str) -> JackResult<Self> {
        let len = Arc::new(AtomicUsize::new(0));
        let (p, c) = bounded_spsc_queue::make(128);
        let mut conn = JackConnection::connect(name)?;
        let dctx = DeviceContext {
            players: ArrayVec::new(),
            chans: ArrayVec::new(),
            control: c,
            length: len.clone(),
            sample_rate: conn.sample_rate() as u64
        };
        conn.set_handler(dctx)?;
        let conn = match conn.activate() {
            Ok(c) => c,
            Err((_, err)) => return Err(err)
        };
        Ok(EngineContext {
            conn: conn,
            chans: ArrayVec::new(),
            length: len,
            control: p
        })
    }
    fn num_senders(&self) -> usize {
        self.length.load(Relaxed)
    }
    fn new_channel(&mut self, name: &str) -> JackResult<usize> {
        let port = self.conn.register_port(name, PORT_IS_OUTPUT | PORT_IS_TERMINAL)?;
        if self.chans.len() == self.chans.capacity() {
            panic!("too many chans"); // FIXME FIXME FIXME: proper error handling
        }
        self.control.push(AudioThreadCommand::AddChannel(port.clone()));
        self.chans.push(port);
        Ok(self.chans.len()-1)
    }
    fn new_sender(&mut self, sample_rate: u64) -> UsefulSender {
        let (p, c) = bounded_spsc_queue::make(STREAM_BUFFER_SIZE);
        let active = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(false));
        let sync = Arc::new(AtomicBool::new(false));
        let position = Arc::new(AtomicU64::new(0));
        let start_time = Arc::new(AtomicU64::new(0));
        let output_patch = Arc::new(AtomicUsize::new(MAX_CHANS));

        self.control.push(AudioThreadCommand::AddPlayer(Player {
            buf: c,
            sample_rate: sample_rate,
            start_time: start_time.clone(),
            position: position.clone(),
            active: active.clone(),
            alive: alive.clone(),
            sync: sync.clone(),
            output_patch: output_patch.clone()
        }));

        Sender {
            buf: p,
            position: position,
            active: active,
            alive: alive,
            output_patch: output_patch,
            start_time: start_time,
            sync: sync,
            sample_rate: sample_rate,
            original: true
        }
    }
}
enum AudioThreadCommand {
    AddPlayer(Player),
    AddChannel(JackPort),
}

use std::thread;
use std::io::{self, Read};
fn main() {
    let mut ec = EngineContext::new("SQA Engine beta0").unwrap();
    let mut reader = hound::WavReader::open("test.wav").unwrap();
    let mut chans = vec![];
    let mut ctls = vec![];
    for ch in 0..reader.spec().channels {
        let st = format!("channel {}", ch);
        let p = ec.new_channel(&st).unwrap();
        let mut send = ec.new_sender(reader.spec().sample_rate as u64);
        send.set_output_patch(p);
        ctls.push(send.make_plain());
        chans.push((p, send));
    }
    for (i, port) in ec.conn.get_ports(None, None, Some(PORT_IS_INPUT | PORT_IS_PHYSICAL)).unwrap().into_iter().enumerate() {
        if let Some(ch) = chans.get(i) {
            ec.conn.connect_ports(&ec.chans[ch.0], &port).unwrap();
        }
    }
    let thr = thread::spawn(move || {
        let mut idx = 0;
        let mut cnt = 0;
        for samp in reader.samples::<f32>() {
            chans[idx].1.buf().push(samp.unwrap());
            idx += 1;
            cnt += 1;
            if cnt == 500_000 {
                println!("Haha, random buffering fail for 5 seconds!!!");
                ::std::thread::sleep(::std::time::Duration::new(5, 0));
                println!("Alright, panic over.");
            }
            if idx >= chans.len() {
                idx = 0;
            }
        }
    });
    println!("*** Press Enter to begin playback!");
    io::stdin().read(&mut [0u8]).unwrap();
    let time = time::precise_time_ns();
    for ch in ctls.iter_mut() {
        ch.set_start_time(time);
        ch.set_active(true);
    }
    let mut secs = 0;
    loop {
        thread::sleep(::std::time::Duration::new(1, 0));
        secs += 1;
        println!("{}: {} samples", ctls[0].position(), ctls[0].position_samples());
        if secs == 20 {
            println!("Haha, some sadist set ch0's active to false for 5 seconds!!!");
            ctls[0].set_active(false);
        }
        if secs == 25 {
            ctls[0].set_active(true);
            println!("Alright, panic over.");
        }
    }
    thr.join().unwrap();
}
