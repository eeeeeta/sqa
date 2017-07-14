//! Fading audio cues' volumes up'n'down.

use sqa_engine::param::{Parameter, FadeDetails};
use super::{ActionController, EditableAction, PerformExt, AsyncResult, PlaybackState, ActionType, ControllerParams, ParameterError, DurationInfo};
use state::Context;
use errors::BackendResult;
use uuid::Uuid;
use std::time::Duration;
use std::default::Default;
use tokio_core::reactor::Timeout;
use futures::Future;
use super::audio::db_lin;
use sqa_engine::Sender;
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FadeParams {
    pub target: Option<Uuid>,
    pub fades: Vec<(bool, f32)>,
    pub fade_master: (bool, f32),
    pub dur: Duration
}
#[derive(Default)]
pub struct Controller {
    params: FadeParams,
    timeout: AsyncResult<(), ::std::io::Error>,
    durinfo: Option<DurationInfo>
}
impl Controller {
    pub fn new() -> Self {
        Default::default()
    }
}
impl EditableAction for Controller {
    type Parameters = FadeParams;
    fn get_params(&self) -> &FadeParams {
        &self.params
    }
    fn set_params(&mut self, mut params: FadeParams, ctx: ControllerParams) {
        if let Some(tgt) = self.params.target.as_ref() {
            if let Some(tgt) = ctx.ctx.actions.get(tgt) {
                if let ActionType::Audio(ref ctl) = tgt.ctl {
                    if ctl.params.chans.len() > params.fades.len() {
                        let len = params.fades.len();
                        params.fades.extend(::std::iter::repeat((false, 0.0))
                                            .take(ctl.params.chans.len() - len));
                    }
                }
            }
        }
        self.params = params;
    }
}
impl ActionController for Controller {
    fn desc(&self, ctx: &Context) -> String {
        if let Some(tgt) = self.params.target.as_ref() {
            if let Some(tgt) = ctx.actions.get(tgt) {
                if let ActionType::Audio(ref ctl) = tgt.ctl {
                    return format!("Fade {}", tgt.meta.name.as_ref().unwrap_or(&ctl.desc(ctx)));
                }
            }
        }
        format!("Fade [invalid]")
    }
    fn verify_params(&self, ctx: &Context) -> Vec<ParameterError> {
        let mut ret = vec![];
        if let Some(tgt) = self.params.target.as_ref() {
            if let Some(tgt) = ctx.actions.get(tgt) {
                match tgt.ctl {
                    ActionType::Audio(_) => {},
                    _ => {
                        ret.push(ParameterError {
                            name: "target".into(),
                            err: "You must target an audio action.".into()
                        });
                    }
                }
            }
            else {
                ret.push(ParameterError {
                    name: "target".into(),
                    err: "No action with that UUID is present.".into()
                });
            }
        }
        else {
            ret.push(ParameterError {
                name: "target".into(),
                err: "No target is specified.".into()
            });
        }
        if !self.params.fade_master.0 && self.params.fades.iter().fold(true, |acc, elem|
                                         if elem.0 && !acc { true } else { acc }){
            ret.push(ParameterError {
                name: "fades".into(),
                err: "Nothing is being faded.".into()
            });
        }
        ret
    }
    fn poll(&mut self, mut ctx: ControllerParams) -> bool {
        let _ = self.timeout.poll();
        if self.timeout.is_complete() {
            ctx.change_state(PlaybackState::Inactive);
            false
        }
        else {
            true
        }
    }
    fn reset(&mut self, _: ControllerParams) {
        self.timeout = AsyncResult::Empty;
        self.durinfo = None;
    }
    fn execute(&mut self, time: u64, mut ctx: ControllerParams) -> BackendResult<bool> {
        {
            let tgt = ctx.ctx.actions.get_mut(self.params.target.as_ref().unwrap())
                .ok_or("Failed to get action")?;
            let tgt = match tgt.ctl {
                ActionType::Audio(ref mut t) => t,
                _ => bail!("Action was wrong type")
            };
            let tgt = tgt.rd.as_mut()
                .ok_or("Target isn't running or loaded")?;
            if self.params.fade_master.0 {
                if let Some(sdr) = tgt.senders.get_mut(0) {
                    let mut fd = FadeDetails::new(sdr.master_volume().get(time), db_lin(self.params.fade_master.1));
                    fd.set_start_time(time);
                    let secs_component = self.params.dur.as_secs() * ::sqa_engine::ONE_SECOND_IN_NANOSECONDS;
                    let subsec_component = self.params.dur.subsec_nanos() as u64;
                    fd.set_duration(secs_component + subsec_component);
                    fd.set_active(true);
                    sdr.set_master_volume(Box::new(Parameter::LinearFade(fd)));
                }
            }
            for (i, &(enabled, fade)) in self.params.fades.iter().enumerate() {
                if enabled {
                    if let Some(sdr) = tgt.senders.get_mut(i) {
                        let mut fd = FadeDetails::new(sdr.volume().get(time), db_lin(fade));
                        fd.set_start_time(time);
                        let secs_component = self.params.dur.as_secs() * ::sqa_engine::ONE_SECOND_IN_NANOSECONDS;
                        let subsec_component = self.params.dur.subsec_nanos() as u64;
                        fd.set_duration(secs_component + subsec_component);
                        fd.set_active(true);
                        sdr.set_volume(Box::new(Parameter::LinearFade(fd)));
                    }
                }
            }
        }
        let now = Sender::<()>::precise_time_ns();
        let mut positive = true;
        let delta = if time > now {
            positive = false;
            time - now
        } else { now - time };
        let secs = delta / 1_000_000_000;
        let ssn = delta % 1_000_000_000;
        let mut dur = Duration::new(secs, ssn as _);
        if positive {
            dur = self.params.dur + dur;
        }
        else {
            dur = self.params.dur - dur;
        }
        let timeout = Timeout::new(dur, ctx.ctx.handle.as_ref().unwrap())?;
        self.timeout = timeout.perform(&mut ctx);
        self.durinfo = Some(DurationInfo {
            start_time: time,
            est_duration: self.params.dur
        });
        Ok(false)
    }
    fn duration_info(&self) -> Option<DurationInfo> {
        self.durinfo.clone()
    }
}
