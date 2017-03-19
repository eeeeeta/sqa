use futures::Future;
use std::error::Error;
use std::path::PathBuf;
use std::any::Any;
use uuid::Uuid;
use time::Duration;
use std::borrow::Cow;
use sqa_engine::sync::AudioThreadMessage;
use state::Context;
use rosc::OscType;
use errors::*;
use serde::{Serialize, Deserialize};
use std::fmt::Debug;
use serde_json;
mod audio;

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
pub trait OscEditable {
    fn edit(&mut self, path: &str, args: Vec<OscType>) -> BackendResult<()>;
}
pub trait ActionController {
    type Parameters: Serialize + Deserialize + Clone + Debug;

    fn desc(&self) -> String;
    fn get_params(&self) -> &Self::Parameters;
    fn set_params(&mut self, Self::Parameters);
    fn verify_params(&self, ctx: &mut Context) -> Vec<ParameterError>;
    fn load(&mut self, _ctx: &mut Context) -> Result<Option<LoadFuture>, Box<Error>> {
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
    fn accept_audio_message(&mut self, _msg: &mut AudioThreadMessage) -> bool {
        false
    }
}
pub enum ActionType {
    Audio(audio::Controller),
}
pub struct Action {
    state: PlaybackState,
    ctl: ActionType,
    uu: Uuid
}
impl Action {
    pub fn new_audio() -> Self {
        Action {
            state: PlaybackState::Inactive,
            ctl: ActionType::Audio(audio::Controller::new()),
            uu: Uuid::new_v4()
        }
    }
    pub fn accept_audio_message(&mut self, msg: &mut AudioThreadMessage) -> bool {
        unimplemented!()
    }
    pub fn state_change(&mut self, ps: PlaybackState) {
        self.state = ps;
    }
    pub fn get_params(&self) -> BackendResult<String> {
        match self.ctl {
            ActionType::Audio(ref a) => serde_json::to_string(a.get_params()).map_err(|e| e.into())
        }
    }
    pub fn set_params(&mut self, data: &str) -> BackendResult<()> {
        match self.ctl {
            ActionType::Audio(ref mut a) => {
                let data = serde_json::from_str(data)?;
                a.set_params(data);
                Ok(())
            }
        }
    }
    pub fn message(&mut self, msg: Box<Any>) -> Result<(), Box<Error>> {
        unimplemented!()
    }
    pub fn uuid(&self) -> Uuid {
        self.uu
    }
}
