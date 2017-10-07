#![allow(unused_doc_comment)]
error_chain! {
    links {
        Backend(::sqa_backend::errors::BackendError, ::sqa_backend::errors::BackendErrorKind);
    }
    foreign_links {
        Io(::std::io::Error);
    }
}
impl From<::rosc::OscError> for Error {
    fn from(e: ::rosc::OscError) -> Error {
        let e: ::sqa_backend::errors::BackendError = ::sqa_backend::errors::BackendErrorKind::OSC(format!("{:?}", e)).into();
        e.into()
    }
}
