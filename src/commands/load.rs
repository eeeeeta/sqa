use super::prelude::*;
use streamv2::{FileStream, FileStreamX, LiveParameters};
use std::time::Duration;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct StreamInfo {
    pub lp: LiveParameters,
    pub ctl: FileStreamX
}
pub struct FileStreamController<'a> {
    si: &'a mut StreamInfo
}
impl<'a> StreamController for FileStreamController<'a> {
    fn unpause(&mut self) {
        self.si.ctl.unpause();
    }
    fn pause(&mut self) {
        self.si.ctl.pause();
    }
    fn restart(&mut self) {
        self.si.ctl.start();
    }
}
#[derive(Clone)]
pub struct LoadCommand {
    file: Option<String>,
    ident: Option<String>,
    ident_set: bool,
    start: bool,
    pub streams: Vec<StreamInfo>
}
impl LoadCommand {
    pub fn new() -> Self {
        LoadCommand {
            file: None,
            ident: None,
            ident_set: false,
            start: true,
            streams: vec![]
        }
    }
}
impl Command for LoadCommand {
    fn name(&self) -> &'static str { "Load file" }
    fn desc(&self, _: &Context) -> String {
        match self.ident {
            None => format!("Load file <b>{}</b>", desc!(self.file)),
            Some(ref id) => format!("Load file <b>{}</b> as $<b>{}</b>", desc!(self.file), id)
        }
    }
    fn run_state(&self) -> Option<CommandState> {
        if let Some(ref info) = self.streams.get(0) {
            Some(if info.lp.pos == Duration::new(0, 0) && info.lp.active == false {
                CommandState::Loaded
            } else {
                CommandState::Running(info.lp.pos)
            })
        }
        else {
            None
        }
    }
    fn get_hunks(&self) -> Vec<Box<Hunk>> {
        let file_getter = move |selfish: &Self| -> Option<String> {
            selfish.file.as_ref().map(|x| x.clone())
        };
        let file_setter = move |selfish: &mut Self, val: Option<String>| {
            if let Some(val) = val {
                selfish.file = Some(val.clone());
                /* FIXME: find a way to make this work
                if !selfish.ident_set {
                    if let Some(osstr) = ::std::path::Path::new(&val).file_stem() {
                        if let Some(realstr) = osstr.to_str() {
                            selfish.ident = Some(realstr.to_owned());
                        }
                    }
                }*/
            }
            else {
                selfish.file = None;
            }
        };
        let file_egetter = move |selfish: &Self, _: &Context| -> Option<String> {
            if let Some(ref val) = selfish.file {
                let info = FileStream::info(Path::new(val));
                if let Err(e) = info {
                    Some(format!("{}", e))
                }
                else if info.unwrap().sample_rate != 44_100 {
                    Some(format!("SQA only supports files with a samplerate of 44.1kHz."))
                }
                else {
                    None
                }
            }
            else {
                Some(format!("A filename to open is required."))
            }
        };
        let ident_getter = move |selfish: &Self| -> Option<String> {
            selfish.ident.as_ref().map(|x| x.clone())
        };
        let ident_setter = move |selfish: &mut Self, val: Option<String>| {
            if let Some(val) = val {
                selfish.ident = Some(val.clone());
                selfish.ident_set = true;
            }
            else {
                selfish.ident = None;
                selfish.ident_set = false;
            }
        };
        let ident_egetter = move |selfish: &Self, ctx: &Context| -> Option<String> {
            if let Some(ref ident) = selfish.ident {
                if ctx.identifiers.get(ident).is_some() {
                    Some(format!("Identifier ${} is already in use.", selfish.ident.as_ref().unwrap()))
                }
                else {
                    None
                }
            }
            else if selfish.file.is_none() {
                None
            }
            else {
                None
            }
        };
        let start_getter = move |selfish: &Self| -> bool {
            selfish.start
        };
        let start_setter = move |selfish: &mut Self, val: bool| {
            selfish.start = val;
        };
        let start_egetter = move |_: &Self, _: &Context| -> Option<String> {
            None
        };
        vec![
            hunk!(FilePath, "Provide the file path to an audio file.", true, Keys::p, file_getter, file_setter, file_egetter),
            TextHunk::new(format!("as")),
            hunk!(String, "Provide an optional named identifier for the new stream.", false, Keys::a, ident_getter, ident_setter, ident_egetter),
            TextHunk::new(format!("(started: ")),
            hunk!(Checkbox, "Should the stream start playing when this command is executed?", false, Keys::r, start_getter, start_setter, start_egetter),
            TextHunk::new(format!(")"))
        ]
    }
    fn load(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>, uu: Uuid) {
        let file = self.file.clone().unwrap();
        let ident = self.ident.clone();
        let mut path = PathBuf::new();
        path.push(Path::new(&file));
        let streams = FileStream::new(path,
                                      evl.channel(),
                                      uu).unwrap();
        for StreamInfo { ctl, .. } in ::std::mem::replace(&mut self.streams, Vec::new()) {
            ctx.mstr.locate_source(ctl.uuid()).unwrap();
        }
        if let Some(i) = ident {
            ctx.label(Some(i), uu);
        }
        for (i, (fs, fsx)) in streams.into_iter().enumerate() {
            let uu = fsx.uuid();
            ctx.mstr.add_source(Box::new(fs));
            self.streams.push(StreamInfo {
                lp: LiveParameters::new(Duration::new(0, 0), Duration::new(0, 0)),
                ctl: fsx
            });
            let dest = ctx.mstr.ichans[i].1;
            ctx.mstr.wire(uu, dest).unwrap();
        }
    }
    fn execute(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>, uu: Uuid) -> Result<bool, String> {
        if self.streams.get(0).is_none() {
            self.load(ctx, evl, uu);
        }
        if let Some(ref mut info) = self.streams.get_mut(0) {
            if self.start {
                info.ctl.unpause();
            }
        }
        else {
            panic!("woops");
        }
        Ok(false)
    }
    fn sources(&self) -> Vec<Uuid> {
        self.streams.iter().map(|x| x.ctl.uuid()).collect()
    }
    fn can_ctl_stream(&self) -> bool { true }
    fn ctl_stream<'a>(&'a mut self) -> Option<Box<StreamController + 'a>> {
        if self.streams.get_mut(0).is_some() {
            Some(Box::new(FileStreamController { si: self.streams.get_mut(0).unwrap() }))
        }
        else {
            None
        }
    }
}
