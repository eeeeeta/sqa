use super::prelude::*;
use streamv2::db_lin;
use backend::BackendTimeout;
use uuid::Uuid;

const FADER_INTERVAL: u64 = 100;

#[derive(Clone)]
pub struct VolCommand {
    ident: Option<String>,
    vol: f32,
    fade: Option<u64>
}
impl VolCommand {
    pub fn new() -> Self {
        VolCommand {
            ident: None,
            /* note: whatever value is set here does not matter,
               as the VolumeUIController::bind() function overwrites it */
            vol: 1.0,
            fade: None
        }
    }
}
impl Command for VolCommand {
    fn name(&self) -> &'static str { "Set volume of" }
    fn get_hunks(&self) -> Vec<Box<Hunk>> {
        let vol_getter = move |selfish: &Self| -> Option<f32> {
            Some(selfish.vol)
        };
        let vol_setter = move |selfish: &mut Self, val: Option<&f32>| {
            if let Some(val) = val {
                selfish.vol = *val;
            }
            else {
                selfish.vol = 0.0;
            }
        };
        let vol_egetter = move |selfish: &Self, _: &ReadableContext| -> Option<String> {
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
        let ident_setter = move |selfish: &mut Self, val: Option<&String>| {
            if let Some(val) = val {
                selfish.ident = Some(val.clone());
            }
            else {
                selfish.ident = None;
            }
        };
        let ident_egetter = move |selfish: &Self, ctx: &ReadableContext| -> Option<String> {
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
        let fade_setter = move |selfish: &mut Self, val: Option<&u64>| {
            if let Some(val) = val {
                selfish.fade = Some(*val);
            }
            else {
                selfish.fade = None;
            }
        };
        let fade_egetter = move |_: &Self, _: &ReadableContext| -> Option<String> {
            None
        };
        vec![
            GenericHunk::new(HunkTypes::Identifier,
                             "Provide the identifier of a stream.", true,
                             Box::new(ident_getter), Box::new(ident_setter), Box::new(ident_egetter)),
            TextHunk::new(format!("<b>@</b>")),
            GenericHunk::new(HunkTypes::Volume,
                             "Provide a target volume.", true,
                             Box::new(vol_getter), Box::new(vol_setter), Box::new(vol_egetter)),
            TextHunk::new(format!("dB")),
            TextHunk::new(format!("(<b>fade</b>")),
            GenericHunk::new(HunkTypes::Time,
                             "Optionally provide a time (in milliseconds) to fade this change over.", false,
            Box::new(fade_getter), Box::new(fade_setter), Box::new(fade_egetter)),
            TextHunk::new(format!("ms)"))
        ]
    }
    fn execute(&mut self, ctx: &mut WritableContext, evl: &mut EventLoop<WritableContext>) -> Result<(), String> {
        let (ident, target) = (self.ident.take().unwrap(), db_lin(self.vol));
        let uu = ctx.db.resolve_ident(&ident).unwrap().0;
        let mut fsx = ctx.db.control_filestream(&uu).unwrap();
        if let Some(fade_secs) = self.fade {
            LinearFader::register(evl, uu, fade_secs, target);
        }
        else {
            for ch in fsx.iter_mut() {
                ch.set_vol(target);
            }
        }
        Ok(())
    }
}
struct LinearFader {
    fsu: Uuid,
    dur: u64,
    ptn: f64,
    target: f32
}
impl LinearFader {
    fn register(evl: &mut EventLoop<WritableContext>, fsu: Uuid, dur: u64, tgt: f32) {
        let lf = LinearFader { fsu: fsu, dur: dur, target: tgt, ptn: ::time::precise_time_s() };
        evl.timeout_ms(Box::new(lf), FADER_INTERVAL).unwrap();
    }
}
impl BackendTimeout for LinearFader {
    fn execute(&mut self, ctx: &mut WritableContext, _: &mut EventLoop<WritableContext>) -> Option<u64> {
        if let Some(mut fsx) = ctx.db.control_filestream(&self.fsu) {
            let lp = fsx[0].lp();
            let fade_left = lp.vol - self.target;
            if fade_left == 0.0 { return None };
            let pos = ((::time::precise_time_s() - self.ptn) * 1000.0).round() as u64;
            let units_left = (self.dur - pos) / 100;
            if units_left == 0 {
                for ch in fsx.iter_mut() {
                    ch.set_vol(self.target);
                }
                Some(FADER_INTERVAL)
            }
            else {
                for ch in fsx.iter_mut() {
                    ch.set_vol(lp.vol - (fade_left / units_left as f32));
                }
                Some(FADER_INTERVAL)
            }
        }
        else {
            None
        }
    }
}
