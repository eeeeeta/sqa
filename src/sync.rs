//! Objects for asynchronous notification of audio thread events.

use parking_lot::{Mutex, Condvar};
use std::sync::Arc;
use std::time::{Duration, Instant};
use bounded_spsc_queue::{Producer, Consumer};
use super::CONTROL_BUFFER_SIZE;
use bounded_spsc_queue;
use uuid::Uuid;
use thread;

pub enum AudioThreadMessage {
    PlayerAdded(Uuid),
    PlayerRejected(thread::Player),
    PlayerRemoved(thread::Player),
    PlayerInvalidOutpatch(Uuid),
    PlayerBufHalf(Uuid),
    PlayerBufEmpty(Uuid),
    Xrun
}

pub struct AudioThreadHandle {
    inner: Arc<(Mutex<()>, Condvar)>,
    rx: Consumer<AudioThreadMessage>
}
impl AudioThreadHandle {
    pub unsafe fn make() -> (AudioThreadHandle, AudioThreadSender) {
        let (p, c) = bounded_spsc_queue::make(CONTROL_BUFFER_SIZE);
        let arc = Arc::new((Mutex::new(()), Condvar::new()));
        (AudioThreadHandle {
            inner: arc.clone(),
            rx: c
        }, AudioThreadSender {
            inner: arc,
            tx: p,
            written_t: 0,
            cur_t: 1
        })
    }
    pub fn try_recv(&mut self) -> Option<AudioThreadMessage> {
        self.rx.try_pop()
    }
    pub fn recv(&mut self) -> AudioThreadMessage {
        if let Some(x) = self.rx.try_pop() { return x; }
        let mut lock = self.inner.0.lock();
        loop {
            self.inner.1.wait(&mut lock);
            if let Some(x) = self.rx.try_pop() { return x; }
        }
    }
    pub fn wait_until(&mut self, timeout: Instant) -> Option<AudioThreadMessage> {
        if let Some(x) = self.rx.try_pop() { return Some(x); }
        let mut lock = self.inner.0.lock();
        self.inner.1.wait_until(&mut lock, timeout);
        self.rx.try_pop()
    }
    pub fn wait_for(&mut self, timeout: Duration) -> Option<AudioThreadMessage> {
        if let Some(x) = self.rx.try_pop() { return Some(x); }
        let mut lock = self.inner.0.lock();
        self.inner.1.wait_for(&mut lock, timeout);
        self.rx.try_pop()
    }
}
pub struct AudioThreadSender {
    inner: Arc<(Mutex<()>, Condvar)>,
    tx: Producer<AudioThreadMessage>,
    written_t: u64,
    cur_t: u64
}
impl AudioThreadSender {
    #[inline(always)]
    pub fn init(&mut self, t: u64) {
        self.cur_t = t;
    }
    #[inline(always)]
    pub fn send(&mut self, data: AudioThreadMessage) {
        self.written_t = self.cur_t;
        self.tx.try_push(data);
    }
    #[inline(always)]
    pub fn notify(&mut self) {
        if self.written_t == self.cur_t {
            self.inner.1.notify_one();
        }
    }
}
