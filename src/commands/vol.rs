use super::prelude::*;
use streamv2::db_lin;
use backend::{BackendTimeout, BackendSender};
use chrono::Duration;
use state::Message;

const FADER_INTERVAL: u64 = 100;

#[derive(Clone)]
pub struct VolCommand {
    ident: Option<String>,
    vol: f32,
    fade: Option<u64>,
    runtime: Option<Duration>
}
impl VolCommand {
    pub fn new() -> Self {
        VolCommand {
            ident: None,
            /* note: whatever value is set here does not matter,
               as the VolumeUIController::bind() function overwrites it */
            vol: 1.0,
            fade: None,
            runtime: None
        }
    }
}
impl Command for VolCommand {
    fn name(&self) -> &'static str { "Set volume of" }
    fn desc(&self) -> String {
        if let Some(amt) = self.fade {
            format!("Fade volume of <b>{}</b> to <b>{}</b>dB over <b>{}</b>ms", desc!(self.ident), self.vol, amt)
        }
        else {
            format!("Set volume of <b>{}</b> to <b>{}</b>dB", desc!(self.ident), self.vol)
        }
    }
    fn run_state(&self) -> Option<CommandState> {
        if let Some(ref rt) = self.runtime {
            Some(CommandState::Running(rt.clone()))
        }
        else {
            None
        }
    }
    fn get_hunks(&self) -> Vec<Box<Hunk>> {
        let vol_getter = move |selfish: &Self| -> f32 {
            selfish.vol
        };
        let vol_setter = move |selfish: &mut Self, val: f32| {
            selfish.vol = val;
        };
        let vol_egetter = move |selfish: &Self, _: &Context| -> Option<String> {
            if selfish.vol.is_nan() {
                Some(format!("Volume has to be a number! (not NaN)"))
            }
            else {
                None
            }
        };
        let ident_getter = move |selfish: &Self| -> Option<String> {
            selfish.ident.as_ref().map(|x| x.clone())
        };
        let ident_setter = move |selfish: &mut Self, val: Option<String>| {
            if let Some(val) = val {
                selfish.ident = Some(val.clone());
            }
            else {
                selfish.ident = None;
            }
        };
        let ident_egetter = move |selfish: &Self, ctx: &Context| -> Option<String> {
            if let Some(ref ident) = selfish.ident {
                if ctx.db.resolve_ident(ident).is_none() {
                    Some(format!("Identifier ${} does not exist.", selfish.ident.as_ref().unwrap()))
                }
                else {
                    None
                }
            }
            else {
                Some(format!("A target identifier is required."))
            }
        };
        let fade_getter = move |selfish: &Self| -> Option<u64> {
            selfish.fade
        };
        let fade_setter = move |selfish: &mut Self, val: Option<u64>| {
            if let Some(val) = val {
                selfish.fade = Some(val);
            }
            else {
                selfish.fade = None;
            }
        };
        let fade_egetter = move |_: &Self, _: &Context| -> Option<String> {
            None
        };
        vec![
            hunk!(Identifier, "Provide the identifier of a stream.", true, ident_getter, ident_setter, (ident_egetter)),
            TextHunk::new(format!("<b>@</b>")),
            hunk!(Volume, "Provide a target volume.", true, (vol_getter), (vol_setter), (vol_egetter)),
            TextHunk::new(format!("dB")),
            TextHunk::new(format!("(<b>fade</b>")),
            hunk!(Time, "Optionally provide a time (in milliseconds) to fade this change over.", false, (fade_getter), (fade_setter), (fade_egetter)),
            TextHunk::new(format!("ms)"))
        ]
    }
    fn execute(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>, auuid: Uuid) -> Result<bool, String> {
        let (ident, target) = (self.ident.clone().unwrap(), db_lin(self.vol));
        let uu = ctx.db.resolve_ident(&ident).unwrap().0;
        let mut fsx = ctx.db.control_filestream(&uu).unwrap();
        if let Some(fade_secs) = self.fade {
            LinearFader::register(evl, uu, fade_secs, target, auuid);
            self.runtime = Some(Duration::seconds(0));
            Ok(false)
        }
        else {
            for ch in fsx.iter_mut() {
                ch.set_vol(target);
            }
            Ok(true)
        }
    }
}
struct LinearFader {
    fsu: Uuid,
    dur: u64,
    ptn: f64,
    target: f32,
    sender: BackendSender,
    auuid: Uuid
}
impl LinearFader {
    fn register(evl: &mut EventLoop<Context>, fsu: Uuid, dur: u64, tgt: f32, auuid: Uuid) {
        let lf = LinearFader { fsu: fsu, dur: dur, target: tgt, ptn: ::time::precise_time_s(), sender: evl.channel(), auuid: auuid };
        evl.timeout_ms(Box::new(lf), FADER_INTERVAL).unwrap();
    }
}
impl BackendTimeout for LinearFader {
    fn execute(&mut self, ctx: &mut Context, _: &mut EventLoop<Context>) -> Option<u64> {
        if let Some(mut fsx) = ctx.db.control_filestream(&self.fsu) {
            let lp = fsx[0].lp();
            let fade_left = lp.vol - self.target;
            if fade_left == 0.0 { return None };
            let pos = ((::time::precise_time_s() - self.ptn) * 1000.0).round() as u64;
            let units_left = (self.dur.saturating_sub(pos)) / 100;
            if units_left == 0 {
                for ch in fsx.iter_mut() {
                    ch.set_vol(self.target);
                }
                self.sender.send(Message::Update(self.auuid, new_update(move |cmd: &mut VolCommand| {
                    cmd.runtime = None;
                }))).unwrap();
                None
            }
            else {
                for ch in fsx.iter_mut() {
                    ch.set_vol(lp.vol - (fade_left / units_left as f32));
                }
                self.sender.send(Message::Update(self.auuid, new_update(move |cmd: &mut VolCommand| {
                    cmd.runtime = Some(Duration::milliseconds(pos as i64));
                }))).unwrap();
                Some(FADER_INTERVAL)
            }
        }
        else {
            None
        }
    }
}
