error_chain! {
    links {
        Backend(::sqa_backend::errors::BackendError, ::sqa_backend::errors::BackendErrorKind);
    }
    foreign_links {
        Io(::std::io::Error);
    }
}
