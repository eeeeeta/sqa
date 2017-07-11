error_chain! {
    types {
        BackendError, BackendErrorKind, ResultExt, BackendResult;
    }
    links {
        Ffmpeg(::sqa_ffmpeg::Error, ::sqa_ffmpeg::ErrorKind);
        Engine(::sqa_engine::errors::Error, ::sqa_engine::errors::ErrorKind);
        Jack(::sqa_engine::sqa_jack::errors::Error, ::sqa_engine::sqa_jack::errors::ErrorKind);
    }
    foreign_links {
        RmpDecode(::rmp_serde::decode::Error);
        RmpEncode(::rmp_serde::encode::Error);
        Io(::std::io::Error);
        StrParse(::std::string::ParseError);
    }
    errors {
        OSC(t: String) {
            display("OSC error: {}", t)
        }
        Uuid(t: String) {
            display("Error parsing UUID: {}", t)
        }
        Url(t: String) {
            display("Error parsing URL: {}", t)
        }
        UnsupportedOSCCommand(t: String) {
            description("Unsupported OSC command provided")
                display("Unsupported OSC command: {}", t)
        }
        UnsupportedOSCBundle {
            description("OSC bundles are not yet supported.")
        }
        MalformedOSCPath {
            description("The OSC path provided was malformed.")
        }
        OSCWrongArgs(r: &'static str) {
            display("expected a {}", r)
        }
        OSCWrongType(n: usize, r: &'static str) {
            display("Argument {} should be a {}", n, r)
        }
        UnknownOSCPath {
            description("The OSC path provided was unknown.")
        }
        EmptyAsyncResult {
            description("No asynchronous computation was performed.")
        }
        WaitingAsyncResult {
            description("An asynchronous computation had not completed.")
        }
    }
}
impl From<::rosc::OscError> for BackendError {
    fn from(e: ::rosc::OscError) -> BackendError {
        BackendErrorKind::OSC(format!("{:?}", e)).into()
    }
}
impl From<::uuid::ParseError> for BackendError {
    fn from(e: ::uuid::ParseError) -> BackendError {
        BackendErrorKind::Uuid(format!("{:?}", e)).into()
    }
}
impl From<::url::ParseError> for BackendError {
    fn from(e: ::url::ParseError) -> BackendError {
        BackendErrorKind::Url(format!("{:?}", e)).into()
    }
}
