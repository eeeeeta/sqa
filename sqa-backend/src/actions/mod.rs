use futures::Future;
use std::error::Error;
use std::path::PathBuf;
use std::any::Any;
use uuid::Uuid;
use std::borrow::Cow;
use sqa_engine::sync::AudioThreadMessage;
use state::{Context};
use rosc::OscType;
use futures::sync::mpsc::Sender;
use state::IntSender;
use errors::*;
use serde::{Serialize, Deserialize};
use std::fmt::Debug;
use std::time::Duration;

pub mod audio;
pub mod fade;

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
    Active(Option<DurationInfo>),
    Errored(String)
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DurationInfo {
    pub start_time: u64,
    pub est_duration: Duration
}
pub struct ControllerParams<'a> {
    ctx: &'a mut Context,
    internal_tx: &'a IntSender,
    uuid: Uuid
}
pub trait OscEditable {
    fn edit(&mut self, path: &str, args: Vec<OscType>) -> BackendResult<()>;
}
pub trait EditableAction {
    type Parameters: Serialize + for<'de> Deserialize<'de> + Clone + Debug + Default;

    fn get_params(&self) -> &Self::Parameters;
    fn set_params(&mut self, Self::Parameters, ctx: &mut Context);
}
pub trait ActionController {
    fn desc(&self, ctx: &Context) -> String;
    fn verify_params(&self, ctx: &mut Context) -> Vec<ParameterError>;
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
    fn estimated_duration(&self) -> Duration {
        Duration::from_millis(0)
    }
    fn duration_info(&self) -> Option<DurationInfo> {
        None
    }
    fn accept_audio_message(&mut self, _msg: &AudioThreadMessage, _ctx: ControllerParams) -> bool {
        false
    }
}
pub enum ActionType {
    Audio(audio::Controller),
    Fade(fade::Controller),
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ActionParameters {
    Audio(<audio::Controller as EditableAction>::Parameters),
    Fade(<fade::Controller as EditableAction>::Parameters)
}
#[macro_use]
pub mod macros {
    macro_rules! action {
        (mut $a:expr) => {{
            use self::ActionType::*;
            match $a {
                Audio(ref mut a) => a as &mut ActionController,
                Fade(ref mut a) => a as &mut ActionController
            }
        }};
        ($a:expr) => {{
            use self::ActionType::*;
            match $a {
                Audio(ref a) => a as &ActionController,
                Fade(ref a) => a as &ActionController,
            }
        }};
        (params $a:expr) => {{
            use self::ActionType::*;
            match $a {
                Audio(ref a) => ActionParameters::Audio(a.get_params().clone()),
                Fade(ref a) => ActionParameters::Fade(a.get_params().clone())
            }
        }};
    }
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpaqueAction {
    pub state: PlaybackState,
    pub params: ActionParameters,
    pub desc: String,
    pub meta: ActionMetadata,
    pub uu: Uuid
}
impl OpaqueAction {
    pub fn display_name(&self) -> &str {
        match self.meta.name {
            Some(ref dsc) => dsc,
            None => &self.desc
        }
    }
    pub fn typ(&self) -> &str {
        match self.params {
            ActionParameters::Audio(_) => "audio",
            ActionParameters::Fade(_) => "fade"
        }
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ActionMetadata {
    pub name: Option<String>,
    pub prewait: Duration
}
pub struct Action {
    state: PlaybackState,
    ctl: ActionType,
    meta: ActionMetadata,
    uu: Uuid
}
impl Action {
    pub fn new_audio() -> Self {
        Action {
            state: PlaybackState::Inactive,
            ctl: ActionType::Audio(audio::Controller::new()),
            uu: Uuid::new_v4(),
            meta: Default::default()
        }
    }
    pub fn new_fade() -> Self {
        Action {
            state: PlaybackState::Inactive,
            ctl: ActionType::Fade(fade::Controller::new()),
            uu: Uuid::new_v4(),
            meta: Default::default()
        }
    }
    pub fn accept_audio_message(&mut self, ctx: &mut Context, sender: &IntSender, msg: &AudioThreadMessage) -> bool {
        let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
        action!(mut self.ctl).accept_audio_message(msg, cp)
    }
    pub fn accept_load(&mut self, ctx: &mut Context, sender: &IntSender, data: Box<Any>) -> BackendResult<()> {
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
    pub fn load(&mut self, ctx: &mut Context, sender: &IntSender) -> BackendResult<()> {
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
    pub fn reset(&mut self, ctx: &mut Context, sender: &IntSender) -> BackendResult<()> {
        let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
        action!(mut self.ctl).reset(cp);
        self.state = PlaybackState::Inactive;
        Ok(())
    }
    pub fn set_uuid(&mut self, uu: Uuid) {
        self.uu = uu;
    }
    pub fn execute(&mut self, time: u64, ctx: &mut Context, sender: &IntSender) -> BackendResult<()> {
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
                self.state = PlaybackState::Active(None);
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
    pub fn get_data(&mut self, ctx: &mut Context) -> BackendResult<OpaqueAction> {
        self.verify_params(ctx);
        Ok(OpaqueAction {
            state: self.state.clone(),
            params: action!(params self.ctl),
            uu: self.uu,
            meta: self.meta.clone(),
            desc: action!(self.ctl).desc(ctx)
        })
    }
    pub fn verify_params(&mut self, ctx: &mut Context) {
        use self::PlaybackState::*;
        let mut new = None;
        let mut active = false;
        match self.state {
            Unverified(..) | Inactive => new = Some(action!(mut self.ctl).verify_params(ctx)),
            Active(_) => active = true,
            _ => {}
        }
        if active {
            let st = action!(self.ctl).duration_info();
            self.state = PlaybackState::Active(st);
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
    pub fn set_meta(&mut self, data: ActionMetadata) {
        self.meta = data; /* neat */
    }
    pub fn set_params(&mut self, data: ActionParameters, ctx: &mut Context) -> BackendResult<()> {
        match self.ctl {
            ActionType::Audio(ref mut a) => {
                if let ActionParameters::Audio(d) = data {
                    a.set_params(d, ctx);
                    Ok(())
                }
                else {
                    bail!("wrong type of action parameters");
                }
            },
            ActionType::Fade(ref mut a) => {
                if let ActionParameters::Fade(d) = data {
                    a.set_params(d, ctx);
                    Ok(())
                }
                else {
                    bail!("wrong type of action parameters");
                }
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
