use super::prelude::*;
use streamv2::FileStreamX;
use state::ObjectType;
#[derive(Clone)]
pub enum StopStartChoice {
    Stop,
    Start,
    ReStart
}
#[derive(Clone)]
pub struct StopStartCommand {
    ident: Option<Uuid>,
    which: StopStartChoice
}
impl StopStartCommand {
    pub fn new(which: StopStartChoice) -> Self {
        StopStartCommand {
            ident: None,
            which: which
        }
    }
}
impl Command for StopStartCommand {
    fn name(&self) -> &'static str {
        match self.which {
            StopStartChoice::Stop => "Stop",
            StopStartChoice::Start => "Start",
            StopStartChoice::ReStart => "Restart"
        }
    }
    fn desc(&self, ctx: &Context) -> String {
        if let Some(ref id) = self.ident {
            format!("{} <b>{}</b>", self.name(), ctx.prettify_uuid(id))
        }
        else {
            format!("{} <b>ALL streams</b>", self.name())
        }
    }
    fn get_hunks(&self) -> Vec<Box<Hunk>> {
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
                    if strm.can_ctl_stream() {
                        None
                    }
                    else {
                        Some(format!("That command isn't a stream."))
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
        let verbiage = match self.which {
            StopStartChoice::Stop => "Provide an identifier to stop, or leave blank to stop all streams.",
            StopStartChoice::Start => "Provide an identifier to start, or leave blank to start all streams.",
            StopStartChoice::ReStart => "Provide an identifier to restart, or leave blank to restart all streams."
        };
        vec![
            hunk!(Identifier, verbiage, false, ident_getter, ident_setter, ident_egetter),
            TextHunk::new(format!("[leave blank for all]"))
        ]
    }
    fn execute(&mut self, ctx: &mut Context, _: &mut EventLoop<Context>, _: Uuid) -> Result<bool, String> {
        for (k, v) in ctx.commands.iter_mut() {
            if let Some(ident) = self.ident {
                if ident != *k { continue }
            }
            if let Some(mut ctl) = v.ctl_stream() {
                match self.which {
                    StopStartChoice::Stop => ctl.pause(),
                    StopStartChoice::Start => ctl.unpause(),
                    StopStartChoice::ReStart => ctl.restart()
                }
            }
        }
        Ok(true)
    }
}
