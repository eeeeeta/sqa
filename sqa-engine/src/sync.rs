//! Objects for asynchronous notification of audio thread events.

use parking_lot::{Mutex, Condvar};
use std::sync::Arc;
use std::time::{Duration, Instant};
use bounded_spsc_queue::{Producer, Consumer};
use super::CONTROL_BUFFER_SIZE;
use bounded_spsc_queue;
use uuid::Uuid;

pub use thread::Player;

/// A message from the audio thread.
pub enum AudioThreadMessage {
    /// The player with a given `Uuid` was successfully added.
    PlayerAdded(Uuid),
    /// This player was rejected due to you exceeding `MAX_PLAYERS`.
    PlayerRejected(Player),
    /// This player was removed on account of not being `alive`.
    PlayerRemoved(Player),
    /// The player with a given `Uuid` has an invalid output patch. Playback has been stopped.
    ///
    /// To resume playback, you MUST change the output patch to a valid channel
    /// number, and call `set_active(true)`.
    PlayerInvalidOutpatch(Uuid),
    /// The player with a given `Uuid`'s buffer is half full.
    ///
    /// This is just to let you know, so that you can start refilling the buffer before it
    /// runs out.
    PlayerBufHalf(Uuid),
    /// The player with a given `Uuid`'s buffer is empty.
    ///
    /// This could simply mean that playback has ended, or it could mean that you didn't
    /// refill the buffer and your audio has now stopped. In the latter case, you OUGHT TO refill the
    /// buffer.
    PlayerBufEmpty(Uuid),
    /// The audio thread has experienced an under- or over- run.
    ///
    /// This REALLY SHOULD NOT happen under normal circumstances. If your sample rate and buffer size
    /// are set to reasonable values, [file a bug!](https://github.com/eeeeeta/sqa-engine).
    /// You MAY WISH TO inform the user that their audio just glitched, and that they should adjust
    /// the sample rate and buffer size to remedy the problem.
    Xrun
}

/// A commmunication channel to receive messages from the audio thread.
pub struct AudioThreadHandle {
    inner: Arc<(Mutex<()>, Condvar)>,
    rx: Consumer<AudioThreadMessage>
}
impl AudioThreadHandle {
    pub(crate) unsafe fn make() -> (AudioThreadHandle, AudioThreadSender) {
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
    /// Attempt to receive a message from the audio thread, returning `None` if none is available.
    pub fn try_recv(&mut self) -> Option<AudioThreadMessage> {
        self.rx.try_pop()
    }
    /// Wait (forever, if necessary) until a message is available, and return it.
    ///
    /// This blocks the thread on a condition variable, consuming no CPU time whilst blocked.
    pub fn recv(&mut self) -> AudioThreadMessage {
        if let Some(x) = self.rx.try_pop() { return x; }
        let mut lock = self.inner.0.lock();
        loop {
            self.inner.1.wait(&mut lock);
            if let Some(x) = self.rx.try_pop() { return x; }
        }
    }
    /// Wait until a message is available, timing out after the specified time instant. Return a
    /// message if obtained in the time period, otherwise `None`.
    ///
    /// The semantics of this function are equivalent to `recv()` except that the thread will be
    /// blocked roughly until the timeout is reached. This method should not be used for precise timing
    /// due to anomalies such as preemption or platform differences that may not cause the maximum
    /// amount of time waited to be precisely the value given.
    ///
    /// Note that the best effort is made to ensure that the time waited is measured with a monotonic
    /// clock, and not affected by the changes made to the system time.
    pub fn wait_until(&mut self, timeout: Instant) -> Option<AudioThreadMessage> {
        if let Some(x) = self.rx.try_pop() { return Some(x); }
        let mut lock = self.inner.0.lock();
        self.inner.1.wait_until(&mut lock, timeout);
        self.rx.try_pop()
    }
    /// Wait until a message is available, timing out after a specified duration. Return a
    /// message if obtained in the time period, otherwise `None`.
    ///
    /// The semantics of this function are equivalent to `recv()` except that the thread will be
    /// blocked for roughly no longer than `timeout`. This method should not be used for precise
    /// timing due to anomalies such as preemption or platform differences that may not cause the
    /// maximum amount of time waited to be precisely `timeout`.
    ///
    /// Note that the best effort is made to ensure that the time waited is measured with a monotonic
    /// clock, and not affected by the changes made to the system time.
    pub fn wait_for(&mut self, timeout: Duration) -> Option<AudioThreadMessage> {
        if let Some(x) = self.rx.try_pop() { return Some(x); }
        let mut lock = self.inner.0.lock();
        self.inner.1.wait_for(&mut lock, timeout);
        self.rx.try_pop()
    }
}
pub(crate) struct AudioThreadSender {
    inner: Arc<(Mutex<()>, Condvar)>,
    tx: Producer<AudioThreadMessage>,
    written_t: u64,
    cur_t: u64
}
impl AudioThreadSender {
    #[inline(always)]
    pub(crate) fn init(&mut self, t: u64) {
        self.cur_t = t;
    }
    #[inline(always)]
    pub(crate) fn send(&mut self, data: AudioThreadMessage) {
        self.written_t = self.cur_t;
        if let Some(remnant) = self.tx.try_push(data) {
            // If we can't send the data to the main thread for deallocation,
            // we don't want to deallocate it in the audio thread!
            ::std::mem::forget(remnant);
        }
    }
    #[inline(always)]
    pub(crate) fn notify(&mut self) {
        if self.written_t == self.cur_t {
            self.inner.1.notify_one();
        }
    }
}
