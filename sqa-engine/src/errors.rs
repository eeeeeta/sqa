use sqa_jack;
error_chain! {
    types {
        Error, ErrorKind, ChainErr, EngineResult;
    }
    links {
        Jack(sqa_jack::errors::Error, sqa_jack::errors::ErrorKind);
    }
    errors {
        LimitExceeded {
            description("You have exceeded the channel or sender limit.")
                display("Engine channel or sender limit exceeded")
        }
    }
}
