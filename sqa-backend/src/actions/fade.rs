//! Fading audio cues' volumes up'n'down.

use sqa_engine::param::{Parameter, FadeDetails};
use super::{ActionController, EditableAction, ActionType, ControllerParams, ParameterError};
use state::Context;
use errors::BackendResult;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FadeParams {
    target: Option<Uuid>,
    fades: Vec<Option<f32>>
}
pub struct Controller {
    params: FadeParams,
    cur_data: Option<FadeDetails<f32>>
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

        Ok(true)
    }
}
