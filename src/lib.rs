#![recursion_limit="4096"]
extern crate ffmpeg_sys;
#[macro_use]
extern crate error_chain;
extern crate libc;
extern crate sample;
extern crate chrono;

pub mod errors;
pub mod frame;
#[macro_use]
mod ffi;

pub use errors::{MediaResult, Error, ErrorKind};
pub use frame::Frame;
pub use chrono::Duration;
use ffmpeg_sys::*;
use std::ptr;
use ffi::str_to_cstr;

/// The sample format of a stream.
#[derive(Copy, Clone, Debug)]
pub enum SampleFormat {
    U8(bool),
    S16(bool),
    S32(bool),
    Float(bool),
    Double(bool),
}

/// One sample, in a given format.
#[derive(Copy, Clone, Debug)]
pub enum Sample {
    U8(u8),
    S16(i16),
    S32(i32),
    Float(libc::c_float),
    Double(libc::c_double)
}

sample_impl! { f32, f64, u8, u16, u32, u64, i8, i16, i32, i64 }

impl SampleFormat {
    pub fn from_ffi(fmt: i32) -> Option<Self> {
        use SampleFormat::*;
        use AVSampleFormat::*;
        match fmt {
            _x if _x == AV_SAMPLE_FMT_U8 as i32 => Some(U8(false)),
            _x if _x == AV_SAMPLE_FMT_S16 as i32 => Some(S16(false)),
            _x if _x == AV_SAMPLE_FMT_S32 as i32 => Some(S32(false)),
            _x if _x == AV_SAMPLE_FMT_FLT as i32 => Some(Float(false)),
            _x if _x == AV_SAMPLE_FMT_DBL as i32 => Some(Double(false)),
            _x if _x == AV_SAMPLE_FMT_U8P as i32 => Some(U8(true)),
            _x if _x == AV_SAMPLE_FMT_S16P as i32 => Some(S16(true)),
            _x if _x == AV_SAMPLE_FMT_S32P as i32 => Some(S32(true)),
            _x if _x == AV_SAMPLE_FMT_FLTP as i32 => Some(Float(true)),
            _x if _x == AV_SAMPLE_FMT_DBLP as i32 => Some(Double(true)),
            _ => None,
        }
    }
    /// Ascertain whether the sample format is planar (that is, non-interleaved).
    pub fn is_planar(&self) -> bool {
        use SampleFormat::*;
        match *self {
            U8(true) => true,
            S16(true) => true,
            S32(true) => true,
            Float(true) => true,
            Double(true) => true,
            _ => false
        }
    }
}
static mut INIT_ONCE: bool = false;
/// FFmpeg context (for thread safety).
pub struct MediaContext {
    net: bool,
    _ptr: *mut () // for !Send and !Sync
}
impl MediaContext {
    pub fn network_init(&mut self) -> MediaResult<()> {
        if self.net {
            bail!(ErrorKind::OnceOnly);
        }
        call!(avformat_network_init());
        self.net = true;
        Ok(())
    }
}
/// Initialise FFmpeg.
pub fn init() -> MediaResult<MediaContext> {
    unsafe {
        if INIT_ONCE {
            bail!(ErrorKind::OnceOnly);
        }
        av_register_all();
        INIT_ONCE = true;
    }
    Ok(MediaContext {
        net: false,
        _ptr: ptr::null_mut()
    })
}
/// A media file, from which you can obtain many `AVFrame`s.
pub struct MediaFile {
    format_ctx: *mut AVFormatContext,
    audio_ctx: *mut AVCodecContext,
}
unsafe impl Send for MediaFile { }
impl MediaFile {
    /// Open a file from the given `url`, which is a [FFmpeg URL]
    /// (https://ffmpeg.org/ffmpeg-protocols.html).
    pub fn new(_ctx: &mut MediaContext, url: &str) -> MediaResult<MediaFile> {
        let url = str_to_cstr(url)?;
        let mut ctx: *mut AVFormatContext = ptr::null_mut();
        call!(avformat_open_input(&mut ctx, url.as_ptr(), ptr::null_mut(), ptr::null_mut()));
        call!(avformat_find_stream_info(ctx, ptr::null_mut()));
        let stream_idx = unsafe {
            av_find_best_stream(ctx, AVMEDIA_TYPE_AUDIO, -1, -1, ptr::null_mut(), 0)
        };
        if stream_idx < 0 {
            Err(ErrorKind::StreamNotFound)?;
        }
        let dec_ctx = unsafe {
            let stream = *(*ctx).streams.offset(stream_idx as isize);
            let dec_ctx = (*stream).codec;
            let decoder = avcodec_find_decoder((*dec_ctx).codec_id);
            if decoder.is_null() {
                Err(ErrorKind::DecoderNotFound)?;
            }
            call!(avcodec_open2(dec_ctx, decoder, ptr::null_mut()));
            dec_ctx
        };
        Ok(MediaFile {
            format_ctx: ctx,
            audio_ctx: dec_ctx,
        })
    }
    fn send_packet(&mut self) -> MediaResult<()> {
        let mut pkt: AVPacket = unsafe { ::std::mem::zeroed() };
        unsafe {
            av_init_packet(&mut pkt);
        }
        call!(av_read_frame(self.format_ctx, &mut pkt));
        call!(avcodec_send_packet(self.audio_ctx, &pkt));
        unsafe {
            av_packet_unref(&mut pkt);
        }
        Ok(())
    }
    fn receive_frame(&mut self) -> MediaResult<Frame> {
        let ptr = unsafe {
            av_frame_alloc()
        };
        let base = unsafe { (*self.audio_ctx).time_base };
        if ptr.is_null() {
            bail!(ErrorKind::AllocationFailed);
        }
        call!(avcodec_receive_frame(self.audio_ctx, ptr));
        Ok(unsafe { Frame::from_ptr(ptr, base)? })
    }
    pub fn channels(&self) -> usize {
        (unsafe { (*self.audio_ctx).channels }) as usize
    }
    pub fn sample_rate(&self) -> usize {
        (unsafe { (*self.audio_ctx).sample_rate }) as usize
    }
    pub fn duration(&self) -> Duration {
        let dur = unsafe { (*self.format_ctx).duration };
        Duration::microseconds(dur)
    }
    pub fn seek(&mut self, to: Duration) -> MediaResult<()> {
        let to = to.num_microseconds().unwrap();
        call!(av_seek_frame(self.format_ctx, -1, to, 0));
        unsafe {
            avcodec_flush_buffers(self.audio_ctx);
        }
        Ok(())
    }
}
impl Iterator for MediaFile {
    type Item = MediaResult<Frame>;
    fn next(&mut self) -> Option<MediaResult<Frame>> {
        loop {
            match self.receive_frame() {
                Ok(frame) => return Some(Ok(frame)),
                Err(a) => {
                    if let ErrorKind::TemporarilyUnavailable = *a.kind() {
                        if let Err(b) = self.send_packet() {
                            if let ErrorKind::EOF = *b.kind() {
                                return None;
                            }
                            return Some(Err(b));
                        }
                    }
                    else if let ErrorKind::EOF = *a.kind() {
                        return None;
                    }
                    else {
                        return Some(Err(a));
                    }
                },
            }
        }
    }
}
