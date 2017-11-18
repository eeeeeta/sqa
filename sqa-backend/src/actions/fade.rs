//! Fading audio cues' volumes up'n'down.

use sqa_engine::param::{Parameter, FadeDetails};
use super::{ActionController, EditableAction, AsyncResult, PlaybackState, ActionType, ControllerParams, ParameterError, DurationInfoInt, DurationInfo};
use async::PerformExt;
use state::Context;
use errors::BackendResult;
use uuid::Uuid;
use std::time::Duration;
use std::default::Default;
use tokio_core::reactor::Timeout;
use futures::Future;
use super::audio::{lin_db, db_lin};
use sqa_engine::Sender;
use std::sync::Arc;
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FadeParams {
    pub target: Option<Uuid>,
    pub fades: Vec<(bool, f32)>,
    pub fade_master: (bool, f32),
    pub dur: Duration
}
struct RunningData {
    params: FadeParams,
    start_time: u64,
    fd: FadeDetails<f32>
}
#[derive(Default)]
pub struct Controller {
    params: FadeParams,
    timeout: AsyncResult<(), ::std::io::Error>,
    rd: Option<RunningData>
}
impl Controller {
    pub fn new() -> Self {
        Default::default()
    }
    fn _apply_fade_to_sdr(&mut self, fade: f32, sdr: &mut Sender<()>, idp: &Arc<()>, time: u64, gt: u64, master: bool) {
        let vol = if master {
            sdr.master_volume().get(gt)
        } else {
            sdr.volume().get(gt)
        };
        let mut fd = FadeDetails::new_with_id(vol, db_lin(fade), idp.clone());
        self.rd.as_mut().unwrap().fd = fd.clone();
        fd.set_duration(self.params.dur);
        fd.start_from_time(time);
        trace!("applying fade [from {:.02}dB to {:.02}dB] to sender, {:.02}% complete already",
               lin_db(vol), fade, 100.0 * fd.percentage_complete(Sender::<()>::precise_time_ns()));
        let bx = Box::new(Parameter::LinearFade(fd));
        if master {
            sdr.set_master_volume(bx)
        }
        else {
            sdr.set_volume(bx)
        }
    }
    fn apply_fade_to_master(&mut self, fade: f32, sdr: &mut Sender<()>, idp: &Arc<()>, time: u64, gt: u64) {
        self._apply_fade_to_sdr(fade, sdr, idp, time, gt, true)
    }
    fn apply_fade_to_sdr(&mut self, fade: f32, sdr: &mut Sender<()>, idp: &Arc<()>, time: u64, gt: u64) {
        self._apply_fade_to_sdr(fade, sdr, idp, time, gt, false)
    }
    fn freeze_sdr(sdr: &mut Sender<()>, fd: &FadeDetails<f32>, orig_t: u64, master: bool) {
        let vol = if master { sdr.master_volume() } else { sdr.volume() };
        let time = Sender::<()>::precise_time_ns();
        if let Parameter::LinearFade(ref f) = vol {
            if f.same_id_as(fd) {
                let val = vol.get(time);
                let before = vol.get(orig_t);
                let thresh = Sender::<()>::precise_time_ns();
                let bx = Box::new(Parameter::TimedRaw(val, thresh+1, before));
                trace!("freezing sender at {:.02}dB: {:?}", lin_db(val), bx);
                if master {
                    sdr.set_master_volume(bx)
                }
                else {
                    sdr.set_volume(bx)
                }
            }
        }
    }
    fn freeze_sdrs(&mut self, ctx: ControllerParams) -> BackendResult<()> {
        if let Some(ref rd) = self.rd {
            let tgt = ctx.ctx.actions.get_mut(rd.params.target.as_ref().unwrap())
                .ok_or("Failed to get action")?;
            let tgt = match tgt.ctl {
                ActionType::Audio(ref mut t) => t,
                _ => bail!("Action was wrong type")
            };
            let tgt = tgt.rd.as_mut()
                .ok_or("Target isn't running or loaded")?;
            for (i, sdr) in tgt.senders.iter_mut().enumerate() {
                trace!("freezing sender #{}", i);
                if i == 0 {
                    Self::freeze_sdr(sdr, &rd.fd, rd.start_time, true);
                }
                Self::freeze_sdr(sdr, &rd.fd, rd.start_time, false);
            }
        }
        Ok(())
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
            trace!("changing state");
            ctx.change_state(PlaybackState::Inactive);
            false
        }
        else {
            true
        }
    }
    fn pause(&mut self, cp: ControllerParams) -> bool {
        if let Ok(_) = self.freeze_sdrs(cp) {
            self.timeout = AsyncResult::Empty;
            true
        }
        else {
            false
        }
    }
    fn reset(&mut self, cp: ControllerParams) {
        self.timeout = AsyncResult::Empty;
        let _ = self.freeze_sdrs(cp);
        self.rd = None;
    }
    fn execute(&mut self, time: u64, mut ctx: ControllerParams) -> BackendResult<bool> {
        {
            let mut gt = time;
            let tgt = ctx.ctx.actions.get_mut(self.params.target.as_ref().unwrap())
                .ok_or("Failed to get action")?;
            let tgt = match tgt.ctl {
                ActionType::Audio(ref mut t) => t,
                _ => bail!("Action was wrong type")
            };
            let tgt = tgt.rd.as_mut()
                .ok_or("Target isn't running or loaded")?;
            let idp = Arc::new(());
            if let Some(ref rd) = self.rd {
                gt = rd.start_time;
                trace!("resuming fade that started @ {}", gt);
            }
            self.rd = Some(RunningData {
                params: self.params.clone(),
                start_time: time,
                fd: FadeDetails::new(0.0, 0.0)
            });
            if self.params.fade_master.0 {
                if let Some(sdr) = tgt.senders.get_mut(0) {
                    let fade = self.params.fade_master.1;
                    trace!("applying fade to master");
                    self.apply_fade_to_master(fade, sdr, &idp, time, gt);
                }
            }
            for (i, (enabled, fade)) in self.params.fades.clone().into_iter().enumerate() {
                if enabled {
                    if let Some(sdr) = tgt.senders.get_mut(i) {
                        trace!("applying fade to chan #{}", i);
                        self.apply_fade_to_sdr(fade, sdr, &idp, time, gt);
                    }
                }
            }
        }
        let now = Sender::<()>::precise_time_ns();
        let mut positive = false;
        let delta = if time > now {
            positive = true;
            time - now
        } else { now - time };
        let secs = delta / 1_000_000_000;
        let ssn = delta % 1_000_000_000;
        let _dur = Duration::new(secs, ssn as _);
        let dur;
        if positive {
            dur = self.params.dur + _dur;
        }
        else {
            if _dur > self.params.dur {
                dur = Duration::new(0, 0);
            }
            else {
                dur = self.params.dur - _dur;
            }
        }
        if dur > Duration::new(0, 0) {
            trace!("time now = {}, sched = {}, delta = {:?}, conf dur = {:?}, wait time = {:?}", now, time, _dur, self.params.dur, dur);
            let timeout = Timeout::new(dur, ctx.ctx.handle.as_ref().unwrap())?;
            self.timeout = timeout.perform(&mut ctx);
            let _ = self.timeout.poll();
            Ok(false)
        }
        else {
            trace!("fade cue with 0 duration");
            Ok(true)
        }
    }
    fn duration_info(&self) -> Option<DurationInfoInt> {
        if let Some(ref rd) = self.rd {
            let now = Sender::<()>::precise_time_ns();
            let start = rd.fd.start_time();
            let delta = if start > now { 0 } else { now - start };
            let elapsed = DurationInfo::nanos_to_dur(delta);
            let total_dur = rd.params.dur;
            Some(DurationInfoInt {
                duration: elapsed,
                start_time: start,
                est_duration: Some(total_dur)
            })
        }
        else {
            None
        }
    }
}
