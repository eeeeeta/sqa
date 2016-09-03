use super::prelude::*;
use mixer::DeviceSink;

#[derive(Clone)]
pub struct OutputCommand {
    chans: Vec<Uuid>
}
impl OutputCommand {
    pub fn new() -> Self {
        OutputCommand {
            chans: Vec::new()
        }
    }
}
impl Command for OutputCommand {
    fn name(&self) -> &'static str { "Initialise default output" }
    fn run_state(&self) -> Option<CommandState> {
        if let Some(_) = self.chans.get(0) {
            Some(CommandState::Loaded)
        }
        else {
            None
        }
    }
    fn get_hunks(&self) -> Vec<Box<Hunk>> {
        vec![TextHunk::new("[TODO]".into())]
    }
    fn load(&mut self, ctx: &mut Context, _: &mut EventLoop<Context>, _: Uuid) {
        let idx = ctx.pa.default_output_device().unwrap();
        let dcs = DeviceSink::from_device_chans(ctx.pa, idx,
                                                ::std::mem::replace(&mut self.chans, Vec::new())).unwrap();
        for (i, dc) in dcs.into_iter().enumerate() {
            let dc: Box<::mixer::Sink> = Box::new(dc);
            self.chans.push(dc.uuid());
            ctx.mstr.add_sink(dc);
            if ctx.mstr.ichans.get(i).is_none() {
                ctx.mstr.add_ich();
                assert!(ctx.mstr.ichans.len() > i);
            }
            let src = ctx.mstr.ichans[i].0;
            ctx.mstr.wire(src, self.chans[i]).unwrap();
        }
    }
    fn unload(&mut self, ctx: &mut Context, _: &mut EventLoop<Context>, _: Uuid) {
        for uu in ::std::mem::replace(&mut self.chans, Vec::new()) {
            ctx.mstr.locate_sink(uu);
        }
    }
    fn execute(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>, uu: Uuid) -> Result<bool, String> {
        if self.chans.get(0).is_none() {
            self.load(ctx, evl, uu);
        }
        Ok(true)
    }
    fn sinks(&self) -> Vec<Uuid> {
        self.chans.clone()
    }
}
