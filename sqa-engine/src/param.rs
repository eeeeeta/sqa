//! Fading the values of parameters over time.

use std::ops::{Sub, Add, Mul};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::atomic::Ordering::*;
use std::sync::Arc;
use std::fmt::Display;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct FadeDetails<T> where T: Mul<f32, Output=T> + Sub<T, Output=T> + Add<T, Output=T> + Copy {
    from: T,
    delta: T,
    start_time: Arc<AtomicU64>,
    duration: Arc<AtomicU64>,
    active: Arc<AtomicBool>,
    id_ptr: Arc<()>
}
#[derive(Clone, Debug)]
pub enum Parameter<T> where T: Mul<f32, Output=T> + Sub<T, Output=T> + Add<T, Output=T> + Copy + Display {
    Raw(T),
    TimedRaw(T, u64, T),
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
            TimedRaw(now, thresh, before) => {
                if time >= thresh { now }
                else { before }
            },
            LinearFade(ref fd) => Self::handle_linear(fd, time)
        }
    }
}
impl<T> FadeDetails<T> where T: Mul<f32, Output=T> + Sub<T, Output=T> + Add<T, Output=T> + Copy {
    fn _new(from: T, to: T, id_ptr: Arc<()>) -> Self {
        let delta = to - from;
        let start_time = Arc::new(AtomicU64::new(0));
        let duration = Arc::new(AtomicU64::new(0));
        let active = Arc::new(AtomicBool::new(false));
        Self { from, delta, start_time, duration, active, id_ptr }
    }
    pub fn new(from: T, to: T) -> Self {
        Self::_new(from, to, Arc::new(()))
    }
    pub fn new_with_id(from: T, to: T, idp: Arc<()>) -> Self {
        Self::_new(from, to, idp)
    }
    pub fn from(&self) -> T {
        self.from
    }
    pub fn set_start_time(&mut self, st: u64) {
        self.start_time.store(st, Relaxed);
    }
    pub fn start_time(&self) -> u64 {
        self.start_time.load(Relaxed)
    }
    pub fn set_duration(&mut self, dur: Duration) {
        let secs_component = dur.as_secs() * super::ONE_SECOND_IN_NANOSECONDS;
        let subsec_component = dur.subsec_nanos() as u64;
        self.set_duration_nanos(secs_component + subsec_component);
    }
    pub fn start_from_time(&mut self, ti: u64) {
        self.set_start_time(ti);
        self.set_active(true);
    }
    pub fn set_duration_nanos(&mut self, st: u64) {
        self.duration.store(st, Relaxed);
    }
    pub fn duration(&self) -> Duration {
        let nanos = self.duration_nanos();
        let secs = nanos / super::ONE_SECOND_IN_NANOSECONDS;
        let ssn = nanos % super::ONE_SECOND_IN_NANOSECONDS;
        Duration::new(secs, ssn as _)
    }
    pub fn duration_nanos(&self) -> u64 {
        self.duration.load(Relaxed)
    }
    pub fn delta(&self) -> T {
        self.delta
    }
    pub fn id_ptr(&self) -> &Arc<()> {
        &self.id_ptr
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
    pub fn same_id_as(&self, fd: &FadeDetails<T>) -> bool {
        Arc::ptr_eq(&self.id_ptr, &fd.id_ptr)
    }
}
