use futures::Future;
use std::error::Error;
use std::path::PathBuf;
use std::any::Any;
use uuid::Uuid;
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
use std::time::Duration;

pub mod audio;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ParameterError {
    pub name: Cow<'static, str>,
    pub err: String
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlaybackState {
    Inactive,
    Unverified(Vec<ParameterError>),
    Loaded,
    Loading,
    Paused,
    Active(Duration),
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
    type Parameters: Serialize + Deserialize + Clone + Debug + Default;

    fn desc(&self) -> String;
    fn get_params(&self) -> &Self::Parameters;
    fn set_params(&mut self, Self::Parameters);
    fn verify_params(&self, ctx: &mut ActionContext) -> Vec<ParameterError>;
    fn load(&mut self, _ctx: ControllerParams) -> BackendResult<bool> {
        Ok(true)
    }
    fn accept_load(&mut self, _ctx: ControllerParams, _data: Box<Any>) -> BackendResult<()> {
        Ok(())
    }
    fn execute(&mut self, time: u64, ctx: ControllerParams) -> BackendResult<bool>;
    fn pause(&mut self, _ctx: ControllerParams) {
    }
    fn reset(&mut self, _ctx: ControllerParams) {
    }
    fn duration(&self) -> Option<Duration> {
        None
    }
    fn accept_audio_message(&mut self, _msg: &AudioThreadMessage, _ctx: ControllerParams) -> bool {
        false
    }
}
pub enum ActionType {
    Audio(audio::Controller),
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ActionParameters {
    Audio(<audio::Controller as ActionController>::Parameters)
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
    }};
    (params $a:expr) => {{
        use self::ActionType::*;
        match $a {
            Audio(ref a) => ActionParameters::Audio(a.get_params().clone())
        }
    }};
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpaqueAction {
    pub state: PlaybackState,
    pub params: ActionParameters,
    pub desc: String,
    pub uu: Uuid
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
    pub fn accept_audio_message(&mut self, ctx: &mut ActionContext, sender: &IntSender, msg: &AudioThreadMessage) -> bool {
        let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
        action!(mut self.ctl).accept_audio_message(msg, cp)
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
            let x = match action!(mut self.ctl).execute(time, cp) {
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
                self.state = PlaybackState::Active(Duration::from_millis(0));
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
    pub fn get_data(&mut self, ctx: &mut ActionContext) -> BackendResult<OpaqueAction> {
        self.verify_params(ctx);
        Ok(OpaqueAction {
            state: self.state.clone(),
            params: action!(params self.ctl),
            uu: self.uu,
            desc: action!(self.ctl).desc()
        })
    }
    pub fn verify_params(&mut self, ctx: &mut ActionContext) {
        use self::PlaybackState::*;
        let mut new = None;
        let mut active = false;
        match self.state {
            Unverified(..) | Inactive => new = Some(action!(self.ctl).verify_params(ctx)),
            Active(_) => active = true,
            _ => {}
        }
        if active {
            let dur = action!(self.ctl).duration();
            if let Some(dur) = dur {
                self.state = PlaybackState::Active(dur);
            }
            else {
                self.state = PlaybackState::Active(Duration::from_millis(0));
            }
        }
        else if let Some(vec) = new {
            if vec.len() == 0 {
                self.state = Inactive;
            }
            else {
                self.state = Unverified(vec)
            }
        }
    }
    pub fn set_params(&mut self, data: ActionParameters) -> BackendResult<()> {
        match self.ctl {
            ActionType::Audio(ref mut a) => {
                let ActionParameters::Audio(d) = data;
                a.set_params(d);
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
