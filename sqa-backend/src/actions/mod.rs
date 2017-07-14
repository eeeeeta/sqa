use futures::{Future, Async, Poll};
use uuid::Uuid;
use std::borrow::Cow;
use sqa_engine::sync::AudioThreadMessage;
use sqa_engine::Sender;
use state::{Context, ServerMessage};
use rosc::OscType;
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
impl DurationInfo {
    pub fn elapsed(&self, rounded: bool) -> (Duration, bool) {
        let now = Sender::<()>::precise_time_ns();
        let mut positive = true;
        let delta = if self.start_time > now {
            positive = false;
            self.start_time - now
        } else { now - self.start_time };
        let mut secs = delta / 1_000_000_000;
        let mut ssn = delta % 1_000_000_000;
        trace!("secs {} ssn {} delta {} now {} start {}", secs, ssn, delta, now, self.start_time);
        if rounded {
            if ssn >= 500_000_000 {
                secs += 1;
            }
            ssn = 0;
        }
        (Duration::new(secs, ssn as _), positive)
    }
}
pub struct ControllerParams<'a> {
    ctx: &'a mut Context,
    internal_tx: &'a IntSender,
    uuid: Uuid
}
impl<'a> ControllerParams<'a> {
    pub fn change_state(&mut self, st: PlaybackState) {
        self.internal_tx.send(ServerMessage::ActionStateChange(self.uuid, st));
    }
    pub fn register_interest(&mut self) {
        self.ctx.async_actions.insert(self.uuid);
    }
    pub fn unregister_interest(&mut self) {
        self.ctx.async_actions.remove(&self.uuid);
    }
}
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
pub trait OscEditable {
    fn edit(&mut self, path: &str, args: Vec<OscType>) -> BackendResult<()>;
}
pub trait EditableAction {
    type Parameters: Serialize + for<'de> Deserialize<'de> + Clone + Debug + Default;

    fn get_params(&self) -> &Self::Parameters;
    fn set_params(&mut self, Self::Parameters, ControllerParams);
}
pub trait ActionController {
    fn desc(&self, ctx: &Context) -> String;
    fn verify_params(&self, ctx: &Context) -> Vec<ParameterError>;
    fn load(&mut self, _ctx: ControllerParams) -> BackendResult<bool> {
        Ok(true)
    }
    fn poll(&mut self, _ctx: ControllerParams) -> bool {
        false
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
    pub fn poll(&mut self, ctx: &mut Context, sender: &IntSender) -> bool {
        let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
        action!(mut self.ctl).poll(cp)
    }
    pub fn execute(&mut self, time: u64, ctx: &mut Context, sender: &IntSender) -> BackendResult<()> {
        let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
        let time = time + (self.meta.prewait.subsec_nanos() as u64) + (self.meta.prewait.as_secs() * 1_000_000_000);
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
    pub fn state_change(&mut self, ps: PlaybackState, ctx: &mut Context, sender: &IntSender) -> BackendResult<()> {
        use self::PlaybackState::*;
        if let Errored(_) = ps {
            /* a state change to Errored is *always* valid */
        }
        else {
            match self.state {
                Inactive | Unverified(_) | Errored(_) => {
                    bail!(format!("A state change (to {:?}) cannot be made whilst {:?}", ps, self.state));
                },
                Loading | Loaded | Active(_) | Paused => {
                    match ps {
                        Loaded => {},
                        Loading => {},
                        Paused => {},
                        Active(_) => {},
                        Inactive => {
                            self.reset(ctx, sender)?;
                            return Ok(())
                        },
                        x => bail!(format!("Wrong state change for {:?}: {:?}", self.state, x))
                    }
                }
            }
        }
        self.state = ps;
        Ok(())
    }
    pub fn get_data(&mut self, ctx: &Context) -> BackendResult<OpaqueAction> {
        self.verify_params(ctx);
        Ok(OpaqueAction {
            state: self.state.clone(),
            params: action!(params self.ctl),
            uu: self.uu,
            meta: self.meta.clone(),
            desc: action!(self.ctl).desc(ctx)
        })
    }
    pub fn verify_params(&mut self, ctx: &Context) {
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
    pub fn set_params(&mut self, data: ActionParameters, ctx: &mut Context, sender: &IntSender) -> BackendResult<()> {
        let ctx: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
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
    pub fn uuid(&self) -> Uuid {
        self.uu
    }
}
