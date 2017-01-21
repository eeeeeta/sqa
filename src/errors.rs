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
        FileNotFound { }
        HttpBadRequest { }
        HttpUnauthorized { }
        HttpNotFound { }
        HttpForbidden { }
        HttpOther4xx { }
        HttpServerError { }
    }
}
