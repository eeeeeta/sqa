use super::prelude::*;
use rsndfile::SndFile;
use streamv2::FileStream;
#[derive(Clone)]
pub struct LoadCommand {
    file: Option<String>,
    ident: Option<String>,
    ident_set: bool
}
impl LoadCommand {
    pub fn new() -> Self {
        LoadCommand {
            file: None,
            ident: None,
            ident_set: false
        }
    }
}
impl Command for LoadCommand {
    fn name(&self) -> &'static str { "Load file" }
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
    fn execute(&mut self, ctx: &mut Context, evl: &mut EventLoop<Context>, uu: Uuid) -> Result<bool, String> {
        let file = self.file.take().ok_or(format!("No filename set."))?;
        let ident = self.ident.take();
        let streams = FileStream::new(SndFile::open(&file)
                                      .map_err(|e| format!("error opening file: {}", e.expl))?,
                                      evl.channel(), uu);
        let uu = ctx.insert_filestream(file, streams);
        ctx.db.get_mut(&uu).unwrap().ident = ident;

        let uuids = ctx.db.get(&uu).unwrap().others.as_ref().unwrap().clone();
        for (i, uid) in uuids.into_iter().enumerate() {
            if let Some(qch) = ctx.db.get_qch(i) {
                ctx.mstr.wire(ctx.db.get(&uid).unwrap().out.as_ref().unwrap().clone(), qch.inp.as_ref().unwrap().clone()).map_err(|e| format!("Wiring failed: {:?}", e))?;
            }
        }
        Ok(false)
    }
}
