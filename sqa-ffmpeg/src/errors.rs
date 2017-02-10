//! Error types & handling.
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
        DevBusy {
            description("Device or resource busy (EBUSY).")
        }
        IOError {
            description("Input/output error (EIO).")
        }
        PermissionDenied {
            description("Permission denied (EACCES).")
        }
        IsADir {
            description("Is a directory (EISDIR).")
        }
        UnsupportedFormat {
            description("The file's sample format is currently unsupported.")
        }
        OnceOnly {
            description("You may only call that function once.")
        }
        TooManySeconds {
            description("You specified a duration that is larger than 9,223,372,036,854,775,807μs.")
        }
        BsfNotFound {
            description("FFmpeg error: Bitstream filter not found")
        }
        FfmpegBug {
            description("FFmpeg error: Internal bug")
        }
        BufferTooSmall {
            description("FFmpeg error: Buffer too small")
        }
        DemuxerNotFound {
            description("FFmpeg error: Decoder not found")
        }
        EncoderNotFound {
            description("FFmpeg error: Demuxer not found")
        }
        ExitRequested {
            description("FFmpeg error: Encoder not found")
        }
        ExternalError {
            description("FFmpeg error: End of file")
        }
        FilterNotFound {
            description("FFmpeg error: Filter not found")
        }
        InvalidData {
            description("FFmpeg error: Invalid data found when processing input")
        }
        MuxerNotFound {
            description("FFmpeg error: Muxer not found")
        }
        OptionNotFound {
            description("FFmpeg error: Option not found")
        }
        PatchWelcome {
            description("FFmpeg error: Not yet implemented in FFmpeg, patches welcome")
        }
        ProtocolNotFound {
            description("FFmpeg error: Protocol not found")
        }
        FfmpegUnknown {
            description("FFmpeg error: Unknown error, typically from an external library")
        }
        FeatureExperimental {
            description("FFmpeg error: Requested feature is flagged experimental. Set strict_std_compliance if you really want to use it.")
        }
        FileNotFound {
            description("No such file or directory (ENOENT).")
        }
        HttpBadRequest {
            description("FFmpeg error: HTTP 400 Bad Request")
        }
        HttpUnauthorized {
            description("FFmpeg error: HTTP 401 Unauthorized")
        }
        HttpNotFound {
            description("FFmpeg error: HTTP 404 Not Found")
        }
        HttpForbidden {
            description("FFmpeg error: HTTP 403 Forbidden")
        }
        HttpOther4xx {
            description("FFmpeg error: Other HTTP 4xx error")
        }
        HttpServerError {
            description("FFmpeg error: HTTP 5xx Server Error")
        }
    }
}
