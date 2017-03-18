error_chain! {
    types {
        BackendError, BackendErrorKind, ResultExt, BackendResult;
    }
    foreign_links {
        Serde(::serde_json::Error);
        Io(::std::io::Error);
    }
    errors {
        OSC(t: String) {
            display("OSC error: {}", t)
        }
        Uuid(t: String) {
            display("Error parsing UUID: {}", t)
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
        UnknownOSCPath {
            description("The OSC path provided was unknown.")
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
