use super::prelude::*;
use streamv2::db_lin;
#[derive(Clone)]
pub struct VolCommand {
    ident: Option<String>,
    vol: f32
}
impl VolCommand {
    pub fn new() -> Self {
        VolCommand {
            ident: None,
            vol: 1.0
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

        vec![
            GenericHunk::new(HunkTypes::Identifier,
                             "Provide the identifier of a stream.", true,
                             Box::new(ident_getter), Box::new(ident_setter), Box::new(ident_egetter)),
            TextHunk::new(format!("<b>@</b>")),
            GenericHunk::new(HunkTypes::Volume,
                             "Provide a target volume.", true,
                             Box::new(vol_getter), Box::new(vol_setter), Box::new(vol_egetter)),
            TextHunk::new(format!("decibels"))
        ]
    }
    fn execute(&mut self, ctx: &mut WritableContext) -> Result<(), String> {
        let (ident, target) = (self.ident.take().unwrap(), db_lin(self.vol));
        let uu = ctx.db.resolve_ident(&ident).unwrap().0;
        let mut fsx = ctx.db.control_filestream(&uu).unwrap();
        for ch in fsx.iter_mut() {
            ch.set_vol(target);
        }
        Ok(())
    }
}
