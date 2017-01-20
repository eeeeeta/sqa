#![recursion_limit="4096"]
extern crate ffmpeg_sys;
#[macro_use]
extern crate error_chain;
extern crate libc;
extern crate sample;
extern crate sqa_engine;

pub mod errors {
    error_chain! {
        types {
            Error, ErrorKind, ChainErr, MediaResult;
        }
        errors {
            UnknownErrorCode(from: &'static str, code: i32) {
                description("Unknown error code.")
                    display("Error code {} in {}", code, from)
            }
            NulError {
                description("Encountered a NUL byte in a Rust &str")
            }
            StreamNotFound {
                description("Failed to find an audio stream in the given file.")
            }
            DecoderNotFound {
                description("Failed to find a decoder for the audio stream.")
            }
            AllocationFailed {
                description("An allocation failed.")
            }
            TemporarilyUnavailable {
                description("Resource temporarily unavailable (EAGAIN).")
            }
            ProgrammerError {
                description("Programmer error: this should never happen (possibly EINVAL).")
                    display("A programmer somewhere has made a mistake")
            }
            EOF {
                description("End of file.")
            }
            UnsupportedFormat {
                description("The file's sample format is currently unsupported.")
            }
            BsfNotFound { }
            FfmpegBug { }
            BufferTooSmall { }
            DemuxerNotFound { }
            EncoderNotFound { }
            ExitRequested { }
            ExternalError { }
            FilterNotFound { }
            InvalidData { }
            MuxerNotFound { }
            OptionNotFound { }
            PatchWelcome { }
            ProtocolNotFound { }
            FfmpegUnknown { }
            FeatureExperimental { }
        }
    }
}
pub use errors::{MediaResult, Error, ErrorKind};
use errors::ChainErr;
use ffmpeg_sys::*;
use std::ffi::CString;
use std::ptr;

macro_rules! call {
    ($name:ident($($arg:expr),*)) => {{
        #[allow(unused_unsafe)]
        let ret = unsafe {
            $name($($arg),+)
        };
        if ret < 0 {
            use ErrorKind::*;
            bail!(match ret {
                _x if -_x == libc::EAGAIN => TemporarilyUnavailable,
                _x if -_x == libc::ENOMEM => AllocationFailed,
                _x if -_x == libc::EINVAL => ProgrammerError,
                AVERROR_EOF => EOF,
                AVERROR_BSF_NOT_FOUND => BsfNotFound,
                AVERROR_BUG => FfmpegBug,
                AVERROR_BUG2 => FfmpegBug,
                _ => UnknownErrorCode(stringify!($name), ret)
            });
        }
        ret
    }};
}
/// Helper function to convert Rust `&str`s to `CString`s.
fn str_to_cstr(st: &str) -> MediaResult<CString> {
    Ok(CString::new(st).chain_err(|| ErrorKind::NulError)?)
}
#[derive(Copy, Clone, Debug)]
enum SampleFormat {
    U8(bool),
    S16(bool),
    S32(bool),
    Float(bool),
    Double(bool),
}
#[derive(Copy, Clone, Debug)]
enum Sample {
    U8(u8),
    S16(i16),
    S32(i32),
    Float(f32),
    Double(f64)
}
use sample::{Sample as SampleSample, ToSample};
impl Sample {
    fn to_f32(self) -> f32 {
        use Sample::*;
        match self {
            U8(x) => x.to_sample(),
            S16(x) => x.to_sample(),
            S32(x) => x.to_sample(),
            Float(x) => x.to_sample(),
            Double(x) => x.to_sample(),
        }
    }
}
impl SampleFormat {
    fn from_ffi(fmt: i32) -> Option<Self> {
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
    fn is_planar(&self) -> bool {
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
#[derive(Debug)]
struct Frame {
    ptr: *mut AVFrame,
    cur_chan: usize,
    cur_idx: usize,
    cap: usize,
    chans: usize,
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
    unsafe fn from_ptr(ptr: *mut AVFrame) -> MediaResult<Self> {
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
        Ok(Frame {
            ptr: ptr,
            cur_chan: 0,
            cur_idx: 0,
            format: format,
            cap: cap as usize,
            chans: chans as usize
        })
    }
    fn capacity(&self) -> usize {
        self.cap
    }
    fn channels(&self) -> usize {
        self.chans
    }
    fn format(&self) -> SampleFormat {
        self.format
    }
    fn set_chan(&mut self, ch: usize) -> bool {
        if ch >= self.chans {
            false
        }
        else {
            self.cur_chan = ch;
            self.cur_idx = 0;
            true
        }
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
pub struct MediaContext {
    _ptr: *mut () // for !Send and !Sync
}
pub fn init() -> MediaContext {
    unsafe {
        av_register_all()
    }
    MediaContext {
        _ptr: ptr::null_mut()
    }
}
struct MediaFile {
    format_ctx: *mut AVFormatContext,
    audio_ctx: *mut AVCodecContext,
}
unsafe impl Send for MediaFile { }
impl MediaFile {
    fn new(_ctx: &MediaContext, url: &str) -> MediaResult<MediaFile> {
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
        if ptr.is_null() {
            bail!(ErrorKind::AllocationFailed);
        }
        call!(avcodec_receive_frame(self.audio_ctx, ptr));
        Ok(unsafe { Frame::from_ptr(ptr)? })
    }
    fn channels(&self) -> usize {
        (unsafe { (*self.audio_ctx).channels }) as usize
    }
    fn sample_rate(&self) -> usize {
        (unsafe { (*self.audio_ctx).sample_rate }) as usize
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
use sqa_engine::{EngineContext, jack, Sender};
use std::io::{self, BufRead, Read};
use std::thread;
fn main() {
    let mctx = init();
    println!("Provide a FFmpeg URL:");

    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let mut buffer = String::new();
    stdin.read_line(&mut buffer).unwrap();
    let file = MediaFile::new(&mctx, &buffer.trim()).unwrap();
    let mut ec = EngineContext::new(None).unwrap();
    let mut chans = vec![];
    let mut ctls = vec![];
    for ch in 0..file.channels() {
        let st = format!("channel {}", ch);
        let p = ec.new_channel(&st).unwrap();
        let mut send = ec.new_sender(file.sample_rate() as u64);
        send.set_output_patch(p);
        ctls.push(send.make_plain());
        chans.push((p, send));
    }
    for (i, port) in ec.conn.get_ports(None, None, Some(jack::PORT_IS_INPUT | jack::PORT_IS_PHYSICAL)).unwrap().into_iter().enumerate() {
        if let Some(ch) = ec.chans.get(i) {
            ec.conn.connect_ports(&ch, &port).unwrap();
        }
    }
    println!("Chans: {} Sample rate: {}", file.channels(), file.sample_rate());
    let thr = ::std::thread::spawn(move || {
        for x in file {
            if let Ok(mut x) = x {
                for (i, ch) in chans.iter_mut().enumerate() {
                    x.set_chan(i);
                    for smpl in &mut x {
                       ch.1.buf.push(smpl.to_f32() * 0.5);
                    }
                }
            }
        }
    });
    let time = Sender::<()>::precise_time_ns();
    for ch in ctls.iter_mut() {
        ch.set_start_time(time);
        ch.set_active(true);
    }
    let mut secs = 0;
    loop {
        thread::sleep(::std::time::Duration::new(1, 0));
        secs += 1;
        println!("{}: {} samples - vol {}", ctls[0].position(), ctls[0].position_samples(), ctls[0].volume());
    }
}
