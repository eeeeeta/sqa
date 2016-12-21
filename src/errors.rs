use JackStatus;
error_chain! {
    types {
        Error, ErrorKind, ChainErr, JackResult;
    }
    errors {
        NulError {
            description("NUL byte in client name")
        }
        JackOpenFailed(code: JackStatus) {
            description("Unable to connect to JACK server.")
                display("jack_client_open() failed: code {}", code.bits())
        }
        ProgrammerError {
            description("Programmer error: this should never happen")
                display("A programmer somewhere has made a mistake.")
        }
        InvalidPort {
            description("Invalid port passed to function")
        }
        UnknownErrorCode(from: &'static str, code: i32) {
            description("Unknown error code.")
                display("Error code {} in {}", code, from)
        }
        Activated {
            description("You may not call this function whilst JACK is activated.")
                display("deactivate() must be called before calling this function")
        }
    }
}
