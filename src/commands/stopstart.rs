use super::prelude::*;
#[derive(Copy, Clone)]
pub enum StopStartChoice {
    Stop,
    Unpause,
    Pause,
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
            StopStartChoice::Unpause => "Unpause",
            StopStartChoice::Pause => "Pause",
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
            selfish.ident
        };
        let ident_setter = move |selfish: &mut Self, val: Option<Uuid>| {
            selfish.ident = val;
        };
        let ident_egetter = move |selfish: &Self, ctx: &Context| -> Option<String> {
            if let Some(ref ident) = selfish.ident {
                if let Some(strm) = ctx.commands.get(ident) {
                    if strm.can_ctl_stream() {
                        None
                    }
                    else {
                        Some("That command isn't a stream.".into())
                    }
                }
                else {
                    Some("That UUID doesn't exist.".into())
                }
            }
            else {
                None
            }
        };
        let verbiage = match self.which {
            StopStartChoice::Stop => "Provide an identifier to stop, or leave blank to stop all streams.",
            StopStartChoice::Unpause => "Provide an identifier to unpause, or leave blank to unpause all streams.",
            StopStartChoice::Pause => "Provide an identifier to pause, or leave blank to pause all streams.",
            StopStartChoice::ReStart => "Provide an identifier to restart, or leave blank to restart all streams."
        };
        vec![
            hunk!(Identifier, verbiage, false, ident_getter, ident_setter, ident_egetter),
            TextHunk::new("[leave blank for all]".into())
        ]
    }
    fn execute(&mut self, ctx: &mut Context, _: &mut EventLoop<Context>, _: Uuid) -> Result<bool, String> {
        for (k, v) in &mut ctx.commands {
            if let Some(ident) = self.ident {
                if ident != *k { continue }
            }
            if let Some(mut ctl) = v.ctl_stream() {
                match self.which {
                    StopStartChoice::Stop => ctl.stop(),
                    StopStartChoice::Pause => ctl.pause(),
                    StopStartChoice::Unpause => ctl.unpause(),
                    StopStartChoice::ReStart => ctl.restart()
                }
            }
        }
        Ok(true)
    }
}
