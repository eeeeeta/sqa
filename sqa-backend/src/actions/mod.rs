use futures::Future;
use std::error::Error;
use std::path::PathBuf;
use std::any::Any;
use uuid::Uuid;
use time::Duration;
use std::borrow::Cow;
use sqa_engine::sync::AudioThreadMessage;
use state::{ActionContext};
use rosc::OscType;
use futures::sync::mpsc::Sender;
use state::IntSender;
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
    Loading,
    Paused,
    Active,
    Errored(String)
}
pub struct ControllerParams<'a> {
    ctx: ActionContext<'a>,
    internal_tx: &'a IntSender,
    uuid: Uuid
}
pub trait OscEditable {
    fn edit(&mut self, path: &str, args: Vec<OscType>) -> BackendResult<()>;
}
pub trait ActionController {
    type Parameters: Serialize + Deserialize + Clone + Debug;

    fn desc(&self) -> String;
    fn get_params(&self) -> &Self::Parameters;
    fn set_params(&mut self, Self::Parameters);
    fn verify_params(&self, ctx: ActionContext) -> Vec<ParameterError>;
    fn load(&mut self, _ctx: ControllerParams) -> BackendResult<bool> {
        Ok(true)
    }
    fn execute(&mut self, time: u64, data: Option<Box<Any>>, ctx: ControllerParams) -> BackendResult<bool>;
    fn duration(&self) -> Option<Duration> {
        None
    }
    fn accept_audio_message(&mut self, _msg: &mut AudioThreadMessage) -> bool {
        false
    }
}
pub enum ActionType {
    Audio(audio::Controller),
}
macro_rules! action {
    (mut $a:expr) => {{
        use self::ActionType::*;
        match $a {
            Audio(ref mut a) => a
        }
    }};
    ($a:expr) => {{
        use self::ActionType::*;
        match $a {
            Audio(ref a) => a
        }
    }}
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
    pub fn load(&mut self, ctx: ActionContext, sender: &IntSender) -> BackendResult<bool> {
        action!(mut self.ctl).load(ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu })
    }
    pub fn execute(&mut self, time: u64, ctx: ActionContext, sender: &IntSender) -> BackendResult<bool> {
        action!(mut self.ctl).execute(time, None, ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu })
    }
    pub fn state_change(&mut self, ps: PlaybackState) {
        self.state = ps;
    }
    pub fn get_params(&self) -> BackendResult<String> {
        serde_json::to_string(action!(self.ctl).get_params()).map_err(|e| e.into())
    }
    pub fn verify_params(&self, ctx: ActionContext) -> Vec<ParameterError> {
        action!(self.ctl).verify_params(ctx)
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
