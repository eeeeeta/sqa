//! Types used in the realtime audio thread.

use sqa_jack::*;
use arrayvec::ArrayVec;
use super::{MAX_PLAYERS, MAX_CHANS, ONE_SECOND_IN_NANOSECONDS};
use bounded_spsc_queue::Consumer;
use std::sync::atomic::{AtomicBool, AtomicUsize, AtomicU64, AtomicU32};
use std::sync::atomic::Ordering::*;
use std::sync::Arc;
use uuid::Uuid;
use time;
use sync::AudioThreadSender;
use sync::AudioThreadMessage::*;

/// Holds data about one mono channel of audio, to be played back on the audio thread.
pub struct Player {
    pub buf: Consumer<f32>,
    pub sample_rate: u64,
    pub start_time: Arc<AtomicU64>,
    pub position: Arc<AtomicU64>,
    pub active: Arc<AtomicBool>,
    pub alive: Arc<AtomicBool>,
    pub output_patch: Arc<AtomicUsize>,
    pub volume: Arc<AtomicU32>,
    pub uuid: Uuid
}
impl Drop for Player {
    fn drop(&mut self) {
        self.active.store(false, Relaxed);
        self.alive.store(false, Relaxed);
    }
}

pub enum AudioThreadCommand {
    AddPlayer(Player),
    AddChannel(JackPort),
}

/// A channel in the device context.
pub struct DeviceChannel {
    /// The `JackPort` of the channel.
    port: JackPort,
    /// The time that this channel was last written to.
    /// Used to zero out the channel if it wasn't written to this callback.
    written_t: u64,
    /// The time that this channel was last zeroed out.
    zeroed_t: u64
}
/// Audio thread handler.
pub struct DeviceContext {
    pub players: ArrayVec<[Player; MAX_PLAYERS]>,
    pub chans: ArrayVec<[DeviceChannel; MAX_CHANS]>,
    pub control: Consumer<AudioThreadCommand>,
    pub length: Arc<AtomicUsize>,
    pub sender: AudioThreadSender,
    pub sample_rate: u64
}
impl DeviceContext {
    #[inline(always)]
    fn handle(&mut self, cmd: AudioThreadCommand) {
        match cmd {
            AudioThreadCommand::AddPlayer(p) => {
                let uu = p.uuid;
                if let Some(p) = self.players.push(p) {
                    self.sender.send(PlayerRejected(p));
                }
                else {
                    let len = self.length.load(Acquire);
                    self.length.store(len + 1, Release);
                    self.players[self.players.len()-1].alive.store(true, Release);
                    self.sender.send(PlayerAdded(uu));
                }
            },
            AudioThreadCommand::AddChannel(p) => {
                self.chans.push(DeviceChannel { port: p, written_t: 0, zeroed_t: 0 });
            }
        }
    }
}
impl JackHandler for DeviceContext {
    #[inline(always)]
    fn xrun(&mut self) -> JackControl {
        self.sender.init(0);
        self.sender.send(Xrun);
        self.sender.notify();
        JackControl::Continue
    }
    #[inline(always)]
    fn process(&mut self, out: &JackCallbackContext) -> JackControl {
        let time = time::precise_time_ns();
        self.sender.init(time);
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
                self.sender.send(PlayerInvalidOutpatch(player.uuid));
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
                self.sender.send(PlayerBufEmpty(player.uuid));
                player.position.store(pos, Relaxed);
                continue;
            }
            if player.buf.size()*2 == player.buf.capacity() {
                self.sender.send(PlayerBufHalf(player.uuid));
            }
            let vol = player.volume.load(Relaxed);
            let vol = unsafe {
                ::std::mem::transmute::<u32, f32>(vol)
            };
            if let Some(buf) = out.get_port_buffer(&self.chans[outpatch].port) {
                let written = time == self.chans[outpatch].written_t;
                if !written {
                    self.chans[outpatch].written_t = time;
                }
                for x in buf.iter_mut() {
                    if let Some(data) = player.buf.try_pop() {
                        if written {
                            *x += data * vol;
                        }
                        else {
                            *x = data * vol;
                        }
                        pos += 1;
                    }
                }
            }
            player.position.store(pos, Relaxed);
        }
        if let Some(x) = to_remove {
            if let Some(p) = self.players.swap_remove(x) {
                self.sender.send(PlayerRemoved(p));
            }
            self.length.store(self.length.load(Relaxed) - 1, Relaxed);
        }
        for ch in self.chans.iter_mut() {
            if ch.written_t != time && ch.zeroed_t < ch.written_t {
                if let Some(buf) = out.get_port_buffer(&ch.port) {
                    for x in buf.iter_mut() {
                        *x = 0.0;
                    }
                }
                ch.zeroed_t = time;
            }
        }
        self.sender.notify();
        JackControl::Continue
    }
}
