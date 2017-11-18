//! Some useful asynchronous primitives.
use errors::*;
use futures::{Async, Poll, Future};
use actions::ControllerParams;

pub enum AsyncResult<T, E> {
    Empty,
    Waiting(Box<Future<Item=T, Error=E>>),
    Result(Result<T, E>)
}
impl<T, E> Default for AsyncResult<T, E> {
    fn default() -> Self {
        AsyncResult::Empty
    }
}
impl<T, E> AsyncResult<T, E> {
    pub fn is_empty(&self) -> bool {
        if let AsyncResult::Empty = *self {
            true
        }
        else {
            false
        }
    }
    pub fn is_waiting(&self) -> bool {
        if let AsyncResult::Waiting(_) = *self {
            true
        }
        else {
            false
        }
    }
    pub fn is_complete(&self) -> bool {
        if let AsyncResult::Result(_) = *self {
            true
        }
        else {
            false
        }
    }
}
impl<T, E> AsyncResult<T, E> where E: Into<BackendError> {
    pub fn as_result(self) -> BackendResult<T> {
        match self {
            AsyncResult::Empty => bail!(BackendErrorKind::EmptyAsyncResult),
            AsyncResult::Waiting(_) => bail!(BackendErrorKind::WaitingAsyncResult),
            AsyncResult::Result(res) => res.map_err(|e| e.into())
        }
    }
}
impl<T, E> Future for AsyncResult<T, E> {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let res: Result<T, E>;
        if let AsyncResult::Waiting(ref mut x) = *self {
            match x.poll() {
                Ok(Async::Ready(t)) => res = Ok(t),
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(e) => res = Err(e)
            }
        }
        else {
            return Ok(Async::Ready(()));
        }
        *self = AsyncResult::Result(res);
        Ok(Async::Ready(()))
    }
}
pub type BackendFuture<T> = Box<Future<Item=T, Error=BackendError>>;
pub trait PerformExt {
    type Item;
    type Error;
    fn perform(self, &mut ControllerParams) -> AsyncResult<Self::Item, Self::Error>;
}
impl<X, T, E> PerformExt for X where X: Future<Item=T, Error=E> + 'static {
    type Item = T;
    type Error = E;
    fn perform(self, p: &mut ControllerParams) -> AsyncResult<T, E> {
        p.register_interest();
        AsyncResult::Waiting(Box::new(self))
    }
}
