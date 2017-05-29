//! Fading audio cues' volumes up'n'down.

use sqa_engine::param::{Parameter, FadeDetails};
use super::{ActionController, EditableAction, ActionType, ControllerParams, ParameterError};
use state::Context;
use errors::BackendResult;
use uuid::Uuid;
use std::time::Duration;
use std::default::Default;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FadeParams {
    target: Option<Uuid>,
    fades: Vec<Option<f32>>,
    dur: Duration
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
    fn set_params(&mut self, params: FadeParams, _: &mut Context) {
        self.params = params;
    }
}
impl ActionController for Controller {
    fn desc(&self) -> String {
        format!("Fade action {:?}", self.params)
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
        if self.params.fades.iter().fold(true, |acc, elem|
                                         if elem.is_some() && !acc { true } else { acc }) {
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
        for (i, fade) in self.params.fades.iter().enumerate() {
            if let Some(fade) = *fade {
                if let Some(sdr) = tgt.senders.get_mut(i) {
                    let mut fd = FadeDetails::new(sdr.volume().get(0), fade);
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
