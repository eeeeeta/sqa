//! Fading audio cues' volumes up'n'down.

use sqa_engine::param::{Parameter, FadeDetails};
use super::{ActionController, EditableAction, ActionType, ControllerParams, ParameterError};
use state::Context;
use errors::BackendResult;
use uuid::Uuid;
use std::time::Duration;
use std::default::Default;
use super::audio::db_lin;
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FadeParams {
    pub target: Option<Uuid>,
    pub fades: Vec<Option<f32>>,
    pub fade_master: Option<f32>,
    pub dur: Duration
}
#[derive(Default)]
pub struct Controller {
    params: FadeParams,
    cur_data: Option<FadeDetails<f32>>
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
    fn set_params(&mut self, mut params: FadeParams, ctx: &mut Context) {
        if let Some(tgt) = self.params.target.as_ref() {
            if let Some(tgt) = ctx.actions.get(tgt) {
                if let ActionType::Audio(ref ctl) = tgt.ctl {
                    if ctl.params.chans.len() > params.fades.len() {
                        let len = params.fades.len();
                        params.fades.extend(::std::iter::repeat(None)
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
    fn verify_params(&self, ctx: &mut Context) -> Vec<ParameterError> {
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
        if self.params.fade_master.is_none() && self.params.fades.iter().fold(true, |acc, elem|
                                         if elem.is_some() && !acc { true } else { acc }){
            ret.push(ParameterError {
                name: "fades".into(),
                err: "Nothing is being faded.".into()
            });
        }
        ret
    }
    fn execute(&mut self, time: u64, ctx: ControllerParams) -> BackendResult<bool> {
        let tgt = ctx.ctx.actions.get_mut(self.params.target.as_ref().unwrap())
            .ok_or("Failed to get action")?;
        let tgt = match tgt.ctl {
            ActionType::Audio(ref mut t) => t,
            _ => bail!("Action was wrong type")
        };
        let tgt = tgt.rd.as_mut()
            .ok_or("Target isn't running or loaded")?;
        if let Some(fade) = self.params.fade_master {
            if let Some(sdr) = tgt.senders.get_mut(0) {
                let mut fd = FadeDetails::new(sdr.volume().get(0), db_lin(fade));
                fd.set_start_time(time);
                let secs_component = self.params.dur.as_secs() * ::sqa_engine::ONE_SECOND_IN_NANOSECONDS;
                let subsec_component = self.params.dur.subsec_nanos() as u64;
                fd.set_duration(secs_component + subsec_component);
                fd.set_active(true);
                sdr.set_master_volume(Box::new(Parameter::LinearFade(fd)));
            }
        }
        for (i, fade) in self.params.fades.iter().enumerate() {
            if let Some(fade) = *fade {
                if let Some(sdr) = tgt.senders.get_mut(i) {
                    let mut fd = FadeDetails::new(sdr.volume().get(0), db_lin(fade));
                    fd.set_start_time(time);
                    let secs_component = self.params.dur.as_secs() * ::sqa_engine::ONE_SECOND_IN_NANOSECONDS;
                    let subsec_component = self.params.dur.subsec_nanos() as u64;
                    fd.set_duration(secs_component + subsec_component);
                    fd.set_active(true);
                    sdr.set_volume(Box::new(Parameter::LinearFade(fd)));
                }
            }
        }
        Ok(true)
    }
}
