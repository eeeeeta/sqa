use super::prelude::*;
use super::LoadCommand;
use streamv2::db_lin;
use backend::{BackendTimeout, BackendSender};
use chrono::Duration;
use state::Message;

const FADER_INTERVAL: u64 = 100;

#[derive(Clone)]
pub struct VolCommand {
    ident: Option<Uuid>,
    vol: f32,
    fade: Option<u64>,
    runtime: Option<Duration>
}
impl VolCommand {
    pub fn new() -> Self {
        VolCommand {
            ident: None,
            vol: 0.0,
            fade: None,
            runtime: None
        }
    }
}
impl Command for VolCommand {
    fn name(&self) -> &'static str { "Set volume of" }
    fn desc(&self, ctx: &Context) -> String {
        if let Some(amt) = self.fade {
            format!("Fade volume of <b>{}</b> to <b>{:.02}</b>dB over <b>{}</b>ms", desc_uuid!(self.ident, ctx), self.vol, amt)
        }
        else {
            format!("Set volume of <b>{}</b> to <b>{:.02}</b>dB", desc_uuid!(self.ident, ctx), self.vol)
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
            selfish.vol = (100.0 * val).round() / 100.0;
        };
        let vol_egetter = move |selfish: &Self, _: &Context| -> Option<String> {
            if selfish.vol.is_nan() {
                Some(format!("Volume has to be a number! (not NaN)"))
            }
            else {
                None
            }
        };
        let ident_getter = move |selfish: &Self| -> Option<Uuid> {
            selfish.ident.as_ref().map(|x| x.clone())
        };
        let ident_setter = move |selfish: &mut Self, val: Option<Uuid>| {
            if let Some(val) = val {
                selfish.ident = Some(val.clone());
            }
            else {
                selfish.ident = None;
            }
        };
        let ident_egetter = move |selfish: &Self, ctx: &Context| -> Option<String> {
            if let Some(ref ident) = selfish.ident {
                if let Some(ref strm) = ctx.commands.get(ident) {
                    if strm.is::<LoadCommand>() {
                        None
                    }
                    else {
                        Some(format!("That command isn't a Load command."))
                    }
                }
                else {
                    Some(format!("That UUID doesn't exist."))
                }
            }
            else {
                None
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
            hunk!(Identifier, "Provide the identifier of a stream.", true, Keys::t, ident_getter, ident_setter, (ident_egetter)),
            TextHunk::new(format!("<b>@</b>")),
            hunk!(Volume, "Provide a target volume.", true, Keys::at, (vol_getter), (vol_setter), (vol_egetter)),
            TextHunk::new(format!("dB")),
            TextHunk::new(format!("(<b>fade</b>")),
            hunk!(Time, "Optionally provide a time (in milliseconds) to fade this change over.", false, Keys::f, (fade_getter), (fade_setter), (fade_egetter)),
            TextHunk::new(format!("ms)"))
        ]
    }
    fn execute(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>, auuid: Uuid) -> Result<bool, String> {
        let (ident, target) = (self.ident.clone().unwrap(), db_lin(self.vol));
        let mut tgt = ctx.commands.get_mut(&ident).unwrap().downcast_mut::<LoadCommand>().unwrap();
        if let Some(fade_secs) = self.fade {
            LinearFader::register(evl, ident, fade_secs, target, auuid);
            self.runtime = Some(Duration::seconds(0));
            Ok(false)
        }
        else {
            for si in tgt.streams.iter_mut() {
                si.ctl.set_vol(target);
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
        if let Some(cmd) = ctx.commands.get_mut(&self.fsu) {
            let ref mut streams = cmd.downcast_mut::<LoadCommand>().unwrap().streams;
            let lp = streams[0].lp;
            let fade_left = lp.vol - self.target;
            if fade_left == 0.0 { return None };
            let pos = ((::time::precise_time_s() - self.ptn) * 1000.0).round() as u64;
            let units_left = (self.dur.saturating_sub(pos)) / 100;
            if units_left == 0 {
                for si in streams {
                    si.ctl.set_vol(self.target);
                }
                self.sender.send(Message::Update(self.auuid, new_update(move |cmd: &mut VolCommand| {
                    cmd.runtime = None;
                    true
                }))).unwrap();
                None
            }
            else {
                for si in streams {
                    si.ctl.set_vol(lp.vol - (fade_left / units_left as f32));
                }
                self.sender.send(Message::Update(self.auuid, new_update(move |cmd: &mut VolCommand| {
                    cmd.runtime = Some(Duration::milliseconds(pos as i64));
                    false
                }))).unwrap();
                Some(FADER_INTERVAL)
            }
        }
        else {
            None
        }
    }
}
