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
            GenericHunk::new(HunkTypes::FilePath,
                             "Provide the file path to an audio file.",
                             Box::new(file_getter), Box::new(file_setter)),
            TextHunk::new(format!("As")),
            GenericHunk::new(HunkTypes::String,
                             "Provide an optional named identifier for the new stream.",
                             Box::new(ident_getter), Box::new(ident_setter))
        ]
    }
    fn get_state(&self, ctx: &ReadableContext) -> CommandState {
        if self.file.is_none() {
            CommandState::bad(format!("Provide a filename to load."))
        }
        else {
            if self.ident.is_some() {
                if ctx.db.resolve_ident(self.ident.as_ref().unwrap()).is_some() {
                    CommandState::bad(format!("Identifier ${} is already in use.", self.ident.as_ref().unwrap()))
                }
                else {
                    CommandState::good(format!("Ready to load file (as ${})", self.ident.as_ref().unwrap()))
                }
            }
            else {
                CommandState::good(format!("Probably ready to load. I haven't bothered to check."))
            }
        }
    }
    fn execute(&mut self, ctx: &mut WritableContext) -> Result<(), String> {
        unimplemented!()
    }
}
