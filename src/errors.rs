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
                display("A programmer somewhere has made a mistake")
        }
        InvalidPort {
            description("Invalid port passed to function")
        }
        PortNotFound {
            description("A port matching that name could not be found.")
        }
        UnknownErrorCode(from: &'static str, code: i32) {
            description("Unknown error code.")
                display("Error code {} in {}", code, from)
        }
        PortRegistrationFailed {
            description("Could not register port (see docs for more details)")
        }
        InvalidPortFlags {
            description("Invalid port passed to function: `from` must be output, `to` must be input")
        }
        InvalidPortType {
            description("Invalid port passed to function: the types of both ports must be equal")
        }
        PortNotMine {
            description("This action requires the port to be owned by the client")
        }
        NotPowerOfTwo {
            description("The new buffer size was not a power of two.")
        }
    }
}
