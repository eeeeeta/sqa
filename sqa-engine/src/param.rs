//! Fading the values of parameters over time.

use std::ops::{Sub, Add, Mul};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::atomic::Ordering::*;
use std::sync::Arc;
use std::fmt::Display;

#[derive(Clone, Debug)]
pub struct FadeDetails<T> where T: Mul<f32, Output=T> + Sub<T, Output=T> + Add<T, Output=T> + Copy {
    from: T,
    delta: T,
    start_time: Arc<AtomicU64>,
    duration: Arc<AtomicU64>,
    active: Arc<AtomicBool>
}
#[derive(Clone, Debug)]
pub enum Parameter<T> where T: Mul<f32, Output=T> + Sub<T, Output=T> + Add<T, Output=T> + Copy + Display {
    Raw(T),
    LinearFade(FadeDetails<T>)
}
impl<T> Parameter<T> where T: Mul<f32, Output=T> + Sub<T, Output=T> + Add<T, Output=T> + Copy + Display {
    pub fn handle_linear(fd: &FadeDetails<T>, time: u64) -> T {
        fd.from() + (fd.delta() * fd.percentage_complete(time))
    }
    pub fn get(&self, time: u64) -> T {
        use self::Parameter::*;
        match *self {
            Raw(ret) => ret,
            LinearFade(ref fd) => Self::handle_linear(fd, time)
        }
    }
}
impl<T> FadeDetails<T> where T: Mul<f32, Output=T> + Sub<T, Output=T> + Add<T, Output=T> + Copy {
    pub fn new(from: T, to: T) -> Self {
        let delta = to - from;
        let start_time = Arc::new(AtomicU64::new(0));
        let duration = Arc::new(AtomicU64::new(0));
        let active = Arc::new(AtomicBool::new(false));
        Self { from, delta, start_time, duration, active }
    }
    pub fn from(&self) -> T {
        self.from
    }
    pub fn set_start_time(&mut self, st: u64) {
        self.start_time.store(st, Relaxed);
    }
    pub fn set_duration(&mut self, st: u64) {
        self.duration.store(st, Relaxed);
    }
    pub fn delta(&self) -> T {
        self.delta
    }
    pub fn set_active(&mut self, active: bool) {
        self.active.store(active, Relaxed);
    }
    pub fn time_elapsed(&self, time: u64) -> u64 {
        let start_time = self.start_time.load(Relaxed);
        time - start_time
    }
    pub fn percentage_complete(&self, time: u64) -> f32 {
        let start_time = self.start_time.load(Relaxed);
        let duration = self.duration.load(Relaxed);
        if time > (start_time + duration) {
            1.0
        }
        else if start_time >= time || !self.active.load(Relaxed) {
            0.0
        }
        else {
            let ns_delta = (time - start_time) as f32;
            let dur = duration as f32;
            ns_delta / dur
        }
    }

}
