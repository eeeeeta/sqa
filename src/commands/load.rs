use super::prelude::*;
use rsndfile::SndFile;
pub struct LoadCommand {
    file: Option<String>,
    ident: Option<String>
}
impl LoadCommand {
    pub fn new() -> Self {
        LoadCommand {
            file: None,
            ident: None
        }
    }
}
impl Command for LoadCommand {
    fn get_hunks(&self) -> Vec<Box<Hunk>> {
        let file_getter = move |selfish: &Self| -> Option<String> {
            selfish.file.as_ref().map(|x| x.clone())
        };
        let file_setter = move |selfish: &mut Self, _: &ReadableContext, val: Option<&String>| {
            if let Some(val) = val {
                let file = SndFile::open(val);
                if let Err(e) = file {
                    Err(format!("Failed to open file: {}", e.expl))
                }
                else if file.as_ref().unwrap().info.samplerate != 44_100 {
                    Err(format!("SQA only supports files with a samplerate of 44.1kHz."))
                }
                else {
                    selfish.file = Some(val.clone());
                    Ok(())
                }
            }
            else {
                selfish.file = None;
                Ok(())
            }
        };
        let ident_getter = move |selfish: &Self| -> Option<String> {
            selfish.ident.as_ref().map(|x| x.clone())
        };
        let ident_setter = move |selfish: &mut Self, ctx: &ReadableContext, val: Option<&String>| {
            if let Some(val) = val {
                if ctx.db.resolve_ident(val).is_some() {
                    Err(format!("Identifier is already in use."))
                }
                else {
                    selfish.ident = Some(val.clone());
                    Ok(())
                }
            }
            else {
                selfish.ident = None;
                Ok(())
            }
        };
        vec![
            GenericHunk::new(HunkTypes::FilePath, Box::new(file_getter), Box::new(file_setter)),
            TextHunk::new(format!("As")),
            GenericHunk::new(HunkTypes::String, Box::new(ident_getter), Box::new(ident_setter))
        ]
    }
    fn get_state(&self, ctx: &ReadableContext) -> CommandState {
        unimplemented!()
    }
    fn execute(&mut self, ctx: &mut WritableContext) -> Result<(), String> {
        unimplemented!()
    }
}
