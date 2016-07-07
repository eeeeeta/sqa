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
    ident: Option<String>,
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
    fn get_hunks(&self) -> Vec<Box<Hunk>> {
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
                None
            }
        };
        let verbiage = match self.which {
            StopStartChoice::Stop => "Provide an identifier to stop, or leave blank to stop all streams.",
            StopStartChoice::Start => "Provide an identifier to start, or leave blank to start all streams.",
            StopStartChoice::ReStart => "Provide an identifier to restart, or leave blank to restart all streams."
        };
        vec![
            GenericHunk::new(HunkTypes::Identifier,
                             verbiage, false,
                             Box::new(ident_getter), Box::new(ident_setter), Box::new(ident_egetter))
        ]
    }
    fn execute(&mut self, ctx: &mut WritableContext, _: &mut EventLoop<WritableContext>) -> Result<(), String> {
        let ident = if let Some(ref id) = self.ident {
            Some(ctx.db.resolve_ident(id).unwrap().0)
        }
        else {
            None
        };
        for ch in ctx.db.iter_mut_type(ObjectType::FileStream(String::new(), 0), ident.as_ref()) {
            let ctl = ch.controller.as_mut().unwrap().downcast_mut::<FileStreamX>().unwrap();
            match self.which {
                StopStartChoice::Stop => ctl.pause(),
                StopStartChoice::Start => ctl.unpause(),
                StopStartChoice::ReStart => ctl.start()
            }
        }
        Ok(())
    }
}
