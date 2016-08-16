use super::prelude::*;
use rsndfile::SndFile;
use streamv2::{FileStream, FileStreamX, LiveParameters};
use chrono::Duration;

#[derive(Clone)]
pub struct StreamInfo {
    pub lp: LiveParameters,
    ctl: FileStreamX
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
    pub streams: Vec<StreamInfo>
}
impl LoadCommand {
    pub fn new() -> Self {
        LoadCommand {
            file: None,
            ident: None,
            ident_set: false,
            streams: vec![]
        }
    }
}
impl Command for LoadCommand {
    fn name(&self) -> &'static str { "Load file" }
    fn desc(&self) -> String {
        format!("Load file <b>{}</b> as <b>{}</b>", desc!(self.file), desc!(self.ident))
    }
    fn run_state(&self) -> Option<CommandState> {
        if let Some(ref info) = self.streams.get(0) {
            Some(if info.lp.pos == 0 && info.lp.active == false {
                CommandState::Loaded
            } else {
                CommandState::Running(Duration::milliseconds((info.lp.pos / 44) as i64))
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
                if !selfish.ident_set {
                    if let Some(osstr) = ::std::path::Path::new(&val).file_stem() {
                        if let Some(realstr) = osstr.to_str() {
                            selfish.ident = Some(realstr.to_owned());
                        }
                    }
                }
            }
            else {
                selfish.file = None;
            }
        };
        let file_egetter = move |selfish: &Self, _: &Context| -> Option<String> {
            if let Some(ref val) = selfish.file {
                let file = SndFile::open(val);
                if let Err(e) = file {
                    Some(format!("Open failed: {}", e.expl))
                }
                else if file.as_ref().unwrap().info.samplerate != 44_100 {
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
                if ctx.db.resolve_ident(ident).is_some() {
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
                Some(format!("Please enter an identifier (we can't guess one)"))
            }
        };
        vec![
            hunk!(FilePath, "Provide the file path to an audio file.", true, file_getter, file_setter, file_egetter),
            TextHunk::new(format!("as")),
            hunk!(String, "Provide an optional named identifier for the new stream.", false, ident_getter, ident_setter, ident_egetter)
        ]
    }
    fn load(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>, uu: Uuid) {
        let file = self.file.clone().unwrap();
        let ident = self.ident.clone();

        let streams = FileStream::new(SndFile::open(&file).unwrap(),
                                      evl.channel(),
                                      uu);
        for StreamInfo { ctl, .. } in ::std::mem::replace(&mut self.streams, Vec::new()) {
            ctx.mstr.locate_source(ctl.uuid()).unwrap();
        }
        if let Some(i) = ident {
            ctx.identifiers.insert(i, uu);
        }
        for (i, (fs, fsx)) in streams.into_iter().enumerate() {
            let uu = fsx.uuid();
            ctx.mstr.add_source(Box::new(fs));
            self.streams.push(StreamInfo {
                lp: LiveParameters::new(0, 0),
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
            info.ctl.start();
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
