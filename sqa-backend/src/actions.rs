use futures::{future, Future};
use std::error::Error;
use std::path::PathBuf;
use std::collections::HashMap;
use std::any::Any;
use std::io;
use uuid::Uuid;
use time::Duration;
pub enum Value {
    Volume(f32),
    Path(PathBuf),
    String(String),
    U32(u32),
    I32(i32)
}

pub struct Parameter<'a> {
    val: Option<&'a Value>,
    desc: &'static str,
    name: &'static str
}
pub struct ParameterError {
    name: &'static str,
    err: String
}
pub enum PlaybackState {
    Inactive,
    Unverified(Vec<ParameterError>),
    Loaded,
    Paused,
    Active,
    Errored(Box<Error>)
}
pub trait ActionController {
    fn desc(&self) -> String;
    fn get_params(&self) -> Vec<Parameter>;
    fn set_param(&mut self, &'static str, Option<Value>) -> bool;
    fn verify_params(&self) -> Vec<ParameterError>;
    fn load(&mut self) -> Box<Future<Item=(), Error=Box<Error>>> {
        Box::new(future::ok(()))
    }
    fn execute(&mut self) -> Box<Future<Item=(), Error=Box<Error>>>;
    fn duration(&self) -> Option<Duration> {
        None
    }
    fn accept_message(&mut self, Box<Any>) -> Result<(), Box<Error>> {
        Err("this ActionController isn't expecting any messages!".into())
    }
}
pub struct Action {
    ctl: Box<ActionController>,
    state: PlaybackState,
    uu: Uuid
}
