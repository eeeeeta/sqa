use futures::Future;
use async::AsyncResult;
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
use tokio_core::reactor::Timeout;

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
    Paused(Option<DurationInfo>),
    Active(Option<DurationInfo>),
    Errored(String)
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct DurationInfo {
    pub elapsed: Duration,
    pub pos: bool,
    pub est_duration: Option<Duration>
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub(crate) struct DurationInfoInt {
    pub duration: Duration,
    pub start_time: u64,
    pub est_duration: Option<Duration>
}
impl DurationInfo {
    pub fn nanos_to_dur(nanos: u64) -> Duration {
        let secs = nanos / 1_000_000_000;
        let ssn = nanos % 1_000_000_000;
        Duration::new(secs, ssn as _)
    }
    pub fn elapsed(&self, rounded: bool) -> (Duration, bool) {
        let mut secs = self.elapsed.as_secs();
        let mut ssn = self.elapsed.subsec_nanos();
        if rounded {
            if ssn >= 500_000_000 {
                secs += 1;
            }
            ssn = 0;
        }
        (Duration::new(secs, ssn), self.pos)
    }
}
impl From<DurationInfoInt> for DurationInfo {
    fn from(v: DurationInfoInt) -> DurationInfo {
        let (elapsed, pos) = v.elapsed(false);
        Self {
            elapsed, pos,
            est_duration: v.est_duration
        }
    }
}
impl DurationInfoInt {
    pub fn elapsed(&self, rounded: bool) -> (Duration, bool) {
        let now = Sender::<()>::precise_time_ns();
        let mut positive = true;
        let mut secs = self.duration.as_secs();
        let mut ssn = self.duration.subsec_nanos();
        if self.start_time > now {
            positive = false;
            let delta = self.start_time - now;
            secs = delta / 1_000_000_000;
            ssn = (delta % 1_000_000_000) as u32;
        }
        if rounded {
            if ssn >= 500_000_000 {
                secs += 1;
            }
            ssn = 0;
        }
        (Duration::new(secs, ssn), positive)
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
        self.ctx.actions.register_interest(self.uuid);
    }
    pub fn unregister_interest(&mut self) {
        self.ctx.actions.unregister_interest(self.uuid);
    }
}
pub trait OscEditable {
    fn edit(&mut self, path: &str, args: Vec<OscType>) -> BackendResult<()>;
}
pub trait EditableAction {
    type Parameters: Serialize + for<'de> Deserialize<'de> + Clone + Debug + Default;

    fn get_params(&self) -> &Self::Parameters;
    fn set_params(&mut self, Self::Parameters, ControllerParams);
}
pub(crate) trait ActionController {
    fn desc(&self, ctx: &Context) -> String;
    fn verify_params(&self, ctx: &Context) -> Vec<ParameterError>;
    fn load(&mut self, _ctx: ControllerParams) -> BackendResult<bool> {
        Ok(true)
    }
    fn poll(&mut self, _ctx: ControllerParams) -> bool {
        false
    }
    fn execute(&mut self, time: u64, ctx: ControllerParams) -> BackendResult<bool>;
    fn pause(&mut self, _ctx: ControllerParams) -> bool {
        false
    }
    fn reset(&mut self, _ctx: ControllerParams) {
    }
    fn estimated_duration(&self) -> Duration {
        Duration::from_millis(0)
    }
    fn duration_info(&self) -> Option<DurationInfoInt> {
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
    pub prewait: Duration,
    pub number: Option<String>
}
pub struct Action {
    state: PlaybackState,
    ctl: ActionType,
    meta: ActionMetadata,
    timeout: AsyncResult<(), ::std::io::Error>,
    uu: Uuid,
    start_asap: bool
}
macro_rules! new_impl {
    ($($aty:ident, $atyl:ident),*) => {
        impl Action {
            $(
                pub fn $atyl() -> Self {
                    Action {
                        state: PlaybackState::Inactive,
                        ctl: ActionType::$aty($atyl::Controller::new()),
                        uu: Uuid::new_v4(),
                        meta: Default::default(),
                        timeout: Default::default(),
                        start_asap: false
                    }
                }
            )*
        }
    }
}
new_impl!(Audio, audio, Fade, fade);
impl Action {
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
    pub fn pause(&mut self, ctx: &mut Context, sender: &IntSender) {
        let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
        let durinfo = if let PlaybackState::Active(_) = self.state {
            action!(self.ctl).duration_info().map(|x| x.into())
        }
        else {
            None
        };
        if action!(mut self.ctl).pause(cp) {
            self.state = PlaybackState::Paused(durinfo);
        }
    }
    pub fn reset(&mut self, ctx: &mut Context, sender: &IntSender) {
        let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
        action!(mut self.ctl).reset(cp);
        self.state = PlaybackState::Inactive;
    }
    pub fn set_uuid(&mut self, uu: Uuid) {
        self.uu = uu;
    }
    pub fn poll(&mut self, ctx: &mut Context, sender: &IntSender) -> bool {
        let _ = self.timeout.poll();
        let mut continue_polling = false;
        if self.timeout.is_complete() {
            //trace!("poll fired; timeout complete");
            if let PlaybackState::Active(_) = self.state {
                let st = action!(self.ctl).duration_info();
                let mut delta_millis;
                if let Some(dur) = st {
                    let (elapsed, pos) = dur.elapsed(false);
                    let elapsed = elapsed.subsec_nanos();
                    let delta_nanos = if pos {
                        1_000_000_000 - elapsed
                    } else { elapsed };
                    delta_millis = delta_nanos / 1_000_000;
                    if delta_millis < 500 {
                        delta_millis = 1000 + delta_millis;
                    }
                    //trace!("elapsed: {:?}; waiting {}ms", elapsed, delta_millis);
                }
                else {
                    delta_millis = 1000;
                }
                self.timeout = AsyncResult::Waiting(
                    Box::new(Timeout::new(Duration::from_millis(delta_millis as _), ctx.handle.as_ref().unwrap()).unwrap())
                );
                let _ = self.timeout.poll();
                continue_polling = true;
            }
            else {
                self.timeout = Default::default();
            }
        }
        else if self.timeout.is_waiting() {
            //trace!("poll fired; timeout still waiting");
            let _ = self.timeout.poll();
            continue_polling = true;
        }
        let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
        let res = action!(mut self.ctl).poll(cp);
        if !continue_polling {
            continue_polling = res;
        }
        continue_polling
    }
    pub fn execute(&mut self, time: u64, ctx: &mut Context, sender: &IntSender) -> BackendResult<()> {
        use self::PlaybackState::*;
        loop {
            match self.state {
                Loading => {
                    self.start_asap = true;
                    break;
                },
                Loaded | Paused(None) => {
                    let time = time + (self.meta.prewait.subsec_nanos() as u64) + (self.meta.prewait.as_secs() * 1_000_000_000);
                    self._execute(time, ctx, sender);
                    break;
                },
                Paused(Some(dur)) => {
                    let delta_nanos = (dur.elapsed.subsec_nanos() as u64) + (dur.elapsed.as_secs() * 1_000_000_000);
                    let time = if dur.pos {
                        time - delta_nanos
                    } else {
                        time + delta_nanos
                    };
                    self._execute(time, ctx, sender);
                    break;
                },
                Active(_) => break,
                Inactive => self.load(ctx, sender)?,
                _ => bail!(format!("Wrong state for executing: expected Loading, Loaded, Inactive, or Active, found {:?}", self.state))
            }
        }
        Ok(())
    }
    fn _execute(&mut self, time: u64, ctx: &mut Context, sender: &IntSender) {
        let x = {
            let cp: ControllerParams = ControllerParams { ctx: ctx, internal_tx: sender, uuid: self.uu };
            match action!(mut self.ctl).execute(time, cp) {
                Ok(b) => b,
                Err(e) => {
                    self.state = PlaybackState::Errored(e.to_string());
                    return;
                }
            }
        };
        if x {
            self.state = PlaybackState::Inactive;
        }
        else {
            ctx.actions.register_interest(self.uu);
            self.timeout = AsyncResult::Waiting(
                Box::new(Timeout::new(Duration::new(1, 0), ctx.handle.as_ref().unwrap()).unwrap())
            );
            let _ = self.timeout.poll();
            self.state = PlaybackState::Active(None);
        }
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
                Loading | Loaded | Active(_) | Paused(_) => {
                    match ps {
                        Loaded => {},
                        Loading => {},
                        Paused(_) => {},
                        Active(_) => {},
                        Inactive => {
                            self.reset(ctx, sender);
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
            let st = action!(self.ctl).duration_info().map(|x| x.into());
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
