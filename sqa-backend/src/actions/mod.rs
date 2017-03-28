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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ParameterError {
    name: Cow<'static, str>,
    err: String
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlaybackState {
    Inactive,
    Unverified(Vec<ParameterError>),
    Loaded,
    Loading,
    Paused,
    Active,
    Errored(String)
}
pub struct ControllerParams<'a, 'b: 'a> {
    ctx: &'a mut ActionContext<'b>,
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
    fn verify_params(&self, ctx: &mut ActionContext) -> Vec<ParameterError>;
    fn load(&mut self, _ctx: ControllerParams) -> BackendResult<bool> {
        Ok(true)
    }
    fn accept_load(&mut self, _ctx: ControllerParams, data: Box<Any>) -> BackendResult<()> {
        Ok(())
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
    pub fn accept_load(&mut self, ctx: &mut ActionContext, sender: &IntSender, data: Box<Any>) -> BackendResult<()> {
        let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
        if let PlaybackState::Loading = self.state {
            match action!(mut self.ctl).accept_load(cp, data) {
                Ok(_) => self.state = PlaybackState::Loaded,
                Err(e) => self.state = PlaybackState::Errored(e.to_string())
            }
        }
        else {
            bail!(format!("Wrong state for accepting load data: expected Loading, found {:?}", self.state));
        }
        Ok(())
    }
    pub fn load(&mut self, ctx: &mut ActionContext, sender: &IntSender) -> BackendResult<()> {
        self.verify_params(ctx);
        let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
        if let PlaybackState::Inactive = self.state {
            let x = match action!(mut self.ctl).load(cp) {
                Ok(b) => b,
                Err(e) => {
                    self.state = PlaybackState::Errored(e.to_string());
                    return Ok(())
                }
            };
            if x {
                self.state = PlaybackState::Loaded;
            }
            else {
                self.state = PlaybackState::Loading;
            }
        }
        else {
            bail!(format!("Wrong state for loading: expected Inactive, found {:?}", self.state));
        }
        Ok(())
    }
    pub fn execute(&mut self, time: u64, ctx: &mut ActionContext, sender: &IntSender) -> BackendResult<()> {
        let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
        if let PlaybackState::Loaded = self.state {
            let x = match action!(mut self.ctl).execute(time, None, cp) {
                Ok(b) => b,
                Err(e) => {
                    self.state = PlaybackState::Errored(e.to_string());
                    return Ok(())
                }
            };
            if x {
                self.state = PlaybackState::Inactive;
            }
            else {
                self.state = PlaybackState::Active;
            }
        }
        else {
            bail!(format!("Wrong state for executing: expected Loaded, found {:?}", self.state));
        }
        Ok(())
    }
    pub fn state_change(&mut self, ps: PlaybackState) {
        self.state = ps;
    }
    pub fn get_data(&mut self, ctx: &mut ActionContext) -> BackendResult<serde_json::Value> {
        self.verify_params(ctx);
        let state = serde_json::to_value(&self.state)?;
        let params = serde_json::to_value(action!(self.ctl).get_params())?;
        Ok(json!({
            "state": state,
            "params": params
        }))
    }
    pub fn verify_params(&mut self, ctx: &mut ActionContext) {
        use self::PlaybackState::*;
        let mut new = None;
        match self.state {
            Unverified(..) | Inactive => new = Some(action!(self.ctl).verify_params(ctx)),
            _ => {}
        }
        if let Some(vec) = new {
            if vec.len() == 0 {
                self.state = Inactive;
            }
            else {
                self.state = Unverified(vec)
            }
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
