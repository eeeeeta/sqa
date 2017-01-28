//! FFI macros and handy functions.
use errors::{MediaResult, ErrorKind, ChainErr};
use std::ffi::CString;
macro_rules! call {
    ($name:ident($($arg:expr),*)) => {{
        #[allow(unused_unsafe)]
        let ret = unsafe {
            $name($($arg),*)
        };
        if ret < 0 {
            use ErrorKind::*;
            bail!(match ret {
                _x if -_x == libc::EAGAIN => TemporarilyUnavailable,
                _x if -_x == libc::ENOMEM => AllocationFailed,
                _x if -_x == libc::EINVAL => ProgrammerError,
                _x if -_x == libc::ENOENT => FileNotFound,
                AVERROR_EOF => EOF,
                AVERROR_BSF_NOT_FOUND => BsfNotFound,
                AVERROR_BUG => FfmpegBug,
                AVERROR_BUG2 => FfmpegBug,
                AVERROR_BUFFER_TOO_SMALL => BufferTooSmall,
                AVERROR_DECODER_NOT_FOUND => DecoderNotFound,
                AVERROR_DEMUXER_NOT_FOUND => DemuxerNotFound,
                AVERROR_ENCODER_NOT_FOUND => EncoderNotFound,
                AVERROR_EXIT => ExitRequested,
                AVERROR_EXTERNAL => ExternalError,
                AVERROR_FILTER_NOT_FOUND => FilterNotFound,
                AVERROR_INVALIDDATA => InvalidData,
                AVERROR_MUXER_NOT_FOUND => MuxerNotFound,
                AVERROR_OPTION_NOT_FOUND => OptionNotFound,
                AVERROR_PATCHWELCOME => PatchWelcome,
                AVERROR_PROTOCOL_NOT_FOUND => ProtocolNotFound,
                AVERROR_STREAM_NOT_FOUND => StreamNotFound,
                AVERROR_UNKNOWN => FfmpegUnknown,
                AVERROR_EXPERIMENTAL => FeatureExperimental,
                AVERROR_HTTP_BAD_REQUEST => HttpBadRequest,
                AVERROR_HTTP_UNAUTHORIZED => HttpUnauthorized,
                AVERROR_HTTP_NOT_FOUND => HttpNotFound,
                AVERROR_HTTP_FORBIDDEN => HttpForbidden,
                AVERROR_HTTP_OTHER_4XX => HttpOther4xx,
                AVERROR_HTTP_SERVER_ERROR => HttpServerError,
                _ => UnknownErrorCode(stringify!($name), ret)
            });
        }
        ret
    }};
}
macro_rules! sample_impl {
    ($($x:ident),+) => {
        use ::sample::{Sample as SampleSample};
        impl Sample {
            $(
            pub fn $x(self) -> $x {
                use Sample::*;
                match self {
                    U8(x) => x.to_sample(),
                    S16(x) => x.to_sample(),
                    S32(x) => x.to_sample(),
                    Float(x) => x.to_sample(),
                    Double(x) => x.to_sample(),
                }
            })*
        }
    };
}
/// Helper function to convert Rust `&str`s to `CString`s.
pub fn str_to_cstr(st: &str) -> MediaResult<CString> {
    Ok(CString::new(st).chain_err(|| ErrorKind::NulError)?)
}
