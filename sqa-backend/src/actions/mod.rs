use futures::{future, Future};
use std::error::Error;
use std::path::PathBuf;
use std::any::Any;
use uuid::Uuid;
use time::Duration;
use std::borrow::Cow;
use sqa_engine::sync::AudioThreadMessage;
use state::Context;

mod audio;

#[derive(Serialize, Deserialize)]
pub enum Value {
    Volume(f32),
    Path(PathBuf),
    String(String),
    U32(u32),
    I32(i32),
    PlaybackState(PlaybackState)
}
macro_rules! value_impl {
    ($impl:ident, $variant:ident, $ty:ty) => {
        impl Value {
            fn $impl(self) -> Option<$ty> {
                match self {
                    Value::$variant(v) => Some(v),
                    _ => None
                }
            }
        }
        impl From<$ty> for Value {
            fn from(v: $ty) -> Self {
                Value::$variant(v)
            }
        }
    }
}
value_impl!(volume, Volume, f32);
value_impl!(path, Path, PathBuf);
value_impl!(string, String, String);
value_impl!(u32, U32, u32);
value_impl!(i32, I32, i32);
value_impl!(playbackstate, PlaybackState, PlaybackState);
#[derive(Serialize, Deserialize)]
pub struct Parameter {
    val: Option<Value>,
    desc: Cow<'static, str>,
    name: Cow<'static, str>
}
#[derive(Serialize, Deserialize)]
pub struct ParameterError {
    name: Cow<'static, str>,
    err: String
}
#[derive(Serialize, Deserialize)]
pub enum PlaybackState {
    Inactive,
    Unverified(Vec<ParameterError>),
    Loaded,
    Paused,
    Active,
    Errored(String)
}
pub type ActionFuture = Box<Future<Item=(), Error=Box<Error>>>;
pub type LoadFuture = Box<Future<Item=Box<Any>, Error=Box<Error + Send>>>;
pub trait ActionController {
    fn desc(&self) -> String;
    fn get_params(&self) -> Vec<Parameter>;
    fn set_param(&mut self, &str, Option<Value>) -> bool;
    fn verify_params(&self, ctx: &mut Context) -> Vec<ParameterError>;
    fn load(&mut self, ctx: &mut Context) -> Result<Option<LoadFuture>, Box<Error>> {
        Ok(None)
    }
    fn loaded(&mut self, &mut Context, Box<Any>) -> Result<(), Box<Error>> {
        Ok(())
    }
    fn execute(&mut self, time: u64, ctx: &mut Context) -> ActionFuture;
    fn duration(&self) -> Option<Duration> {
        None
    }
    fn accept_message(&mut self, Box<Any>) -> Result<(), Box<Error>> {
        Err("this ActionController isn't expecting any messages!".into())
    }
    fn accept_audio_message(&mut self, msg: &mut AudioThreadMessage) -> bool {
        false
    }
}
#[derive(Serialize, Deserialize)]
pub enum ActionType {
    Audio,
    NotAudio
}
pub struct Action {
    ctl: Box<ActionController>,
    state: PlaybackState,
    typ: ActionType,
    uu: Uuid
}
impl Action {
    pub fn accept_audio_message(&mut self, msg: &mut AudioThreadMessage) -> bool {
        if let ActionType::Audio = self.typ {
            self.ctl.accept_audio_message(msg)
        }
        else {
            false
        }
    }
    pub fn state_change(&mut self, ps: PlaybackState) {
        self.state = ps;
    }
    pub fn message(&mut self, msg: Box<Any>) -> Result<(), Box<Error>> {
        self.ctl.accept_message(msg)
    }
}
