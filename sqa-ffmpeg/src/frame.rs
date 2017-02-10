//! Wrapper & iterator for the FFmpeg `AVFrame` type.
use ffmpeg_sys::*;
use errors::{MediaResult, ErrorKind};
use super::{SampleFormat, Sample};
use chrono::Duration;
use libc;
#[derive(Debug)]
pub struct Frame {
    ptr: *mut AVFrame,
    cur_chan: usize,
    cur_idx: usize,
    cap: usize,
    chans: usize,
    pts: libc::c_double,
    format: SampleFormat
}
impl Drop for Frame {
    fn drop(&mut self) {
        unsafe {
            av_frame_free(&mut self.ptr);
        }
    }
}
impl Frame {
    pub unsafe fn from_ptr(ptr: *mut AVFrame, time: AVRational) -> MediaResult<Self> {
        let format = (*ptr).format;
        let format = if let Some(x) = SampleFormat::from_ffi(format) { x }
        else {
            bail!(ErrorKind::UnsupportedFormat)
        };
        let mut cap = (*ptr).nb_samples;
        let chans = (*ptr).channels;
        if !format.is_planar() {
            cap *= chans;
        }
        let pts = (*ptr).pts as libc::c_double;
        let time = av_q2d(time);
        Ok(Frame {
            ptr: ptr,
            cur_chan: 0,
            cur_idx: 0,
            format: format,
            pts: pts * time,
            cap: cap as usize,
            chans: chans as usize
        })
    }
    pub fn capacity(&self) -> usize {
        self.cap
    }
    pub fn channels(&self) -> usize {
        self.chans
    }
    pub fn format(&self) -> SampleFormat {
        self.format
    }
    pub fn set_chan(&mut self, ch: usize) -> bool {
        if ch >= self.chans {
            false
        }
        else {
            self.cur_chan = ch;
            self.cur_idx = 0;
            true
        }
    }
    pub fn pts(&self) -> Duration {
        Duration::nanoseconds((1_000_000_000f64 * self.pts) as _)
    }
}
impl<'a> Iterator for &'a mut Frame {
    type Item = Sample;
    fn next(&mut self) -> Option<Sample> {
        if self.cur_idx >= self.cap {
            return None;
        }
        let chan;
        if self.format.is_planar() {
            chan = self.cur_chan;
        }
        else {
            chan = 0;
            if self.cur_idx == 0 {
                self.cur_idx += self.cur_chan;
            }
        }
        unsafe {
            let data = *(*self.ptr).extended_data.offset(chan as isize);
            let ret = Some(match self.format {
                SampleFormat::U8(_) => Sample::U8(*(data as *mut _).offset(self.cur_idx as isize)),
                SampleFormat::S16(_) => Sample::S16(*(data as *mut _).offset(self.cur_idx as isize)),
                SampleFormat::S32(_) => Sample::S32(*(data as *mut _).offset(self.cur_idx as isize)),
                SampleFormat::Float(_) => Sample::Float(*(data as *mut _).offset(self.cur_idx as isize)),
                SampleFormat::Double(_) => Sample::Double(*(data as *mut _).offset(self.cur_idx as isize)),
            });
            if self.format.is_planar() {
                self.cur_idx += 1;
            }
            else {
                self.cur_idx += self.chans;
            }
            ret
        }
    }
}
