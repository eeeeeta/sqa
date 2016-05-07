use parser::{Tokens, ParserErr};
use command::Command;
use streamv2::{FileStream, FileStreamX};
use uuid::Uuid;
use state::{ReadableContext, WritableContext, ObjectType, Database};
use rsndfile::SndFile;

pub struct LoadCommand {
    cli: Vec<Tokens>,
    file: Option<SndFile>,
    ident: Option<String>
}
impl LoadCommand {
    pub fn new() -> Self {
        LoadCommand {
            cli: Vec::new(),
            file: None,
            ident: None
        }
    }
}
impl Command for LoadCommand {
    fn execute(&mut self, ctx: &mut WritableContext) {
        let file = self.file.take().unwrap();
        let mut ident = self.ident.take();
        let streams = FileStream::new(file);
        let mut path = None;
        for tok in self.cli.iter() {
            if let &Tokens::Path(ref p) = tok {
                path = Some(p.clone());
                break;
            }
        }
        let filen = path.expect("invariant violated: file, but no Path in cli");
        if ident.is_none() {
            ident = Some(::std::path::Path::new(&filen).file_stem().map(|x| x.to_str()).unwrap().unwrap().to_owned());
        }
        let uu = ctx.insert_filestream(filen, streams);
        ctx.db.get_mut(&uu).unwrap().ident = ident;

        let uuids = ctx.db.get(&uu).unwrap().others.as_ref().unwrap().clone();
        for (i, uid) in uuids.into_iter().enumerate() {
            if let Some(qch) = ctx.db.get_qch(i) {
                ctx.mstr.wire(ctx.db.get(&uid).unwrap().out.as_ref().unwrap().clone(), qch.inp.as_ref().unwrap().clone()).unwrap();
            }
        }
    }
    fn line(&self) -> (Tokens, &Vec<Tokens>) {
        (Tokens::Load, &self.cli)
    }
    fn is_complete(&self, ctx: &ReadableContext) -> Result<(), String> {
        if self.file.is_none() { Err(format!("No file.")) }
        else {
            if self.ident.is_none() {
                let mut path = None;
                for tok in self.cli.iter() {
                    if let &Tokens::Path(ref p) = tok {
                        path = Some(p.clone());
                        break;
                    }
                }
                let filen = path.expect("invariant violated: file, but no Path in cli");
                let ident = ::std::path::Path::new(&filen).file_stem().map(|x| x.to_str());
                if let Some(Some(id)) = ident {
                    if ctx.db.resolve_ident(id).is_some() {
                        Err(format!("The identifier ${} is already in use. Please manually specify an identifier name for this file.", id))
                    }
                    else {
                        Ok(())
                    }

                }
                else {
                    Err(format!("Please manually specify an identifier name for this file."))
                }
            }
            else {
                let self_ident: &str = self.ident.as_ref().unwrap();
                if ctx.db.resolve_ident(self_ident).is_some() {
                    Err(format!("The identifier ${} is already in use. Please specify another.", self_ident))
                }
                else {
                    Ok(())
                }
            }
        }
    }
    fn back(&mut self) -> Option<Tokens> {
        {
            let ld = Tokens::Load; // to appease borrowck
            let last = self.cli.iter().next_back().unwrap_or(&ld);
            match last {
                &Tokens::Load => {},
                &Tokens::Path(_) => { self.file = None },
                &Tokens::As => {},
                &Tokens::Identifier(_) => { self.ident = None },
                _ => unreachable!()
            }
        }
        self.cli.pop()
    }
    fn add(&mut self, tok: Tokens, ctx: &ReadableContext) -> Result<(), ParserErr> {
        let last = self.cli.iter().next_back().unwrap_or(&Tokens::Load).clone();
        match last {
            Tokens::Load => {
                if let Tokens::Path(path) = tok {
                    let file = SndFile::open(&path);
                    if let Err(e) = file {
                        Err(ParserErr::ArgumentError(format!("Failed to open file: {}", e.expl)))
                    }
                    else if file.as_ref().unwrap().info.samplerate != 44_100 {
                        Err(ParserErr::ArgumentError(format!("Sample rate mismatch.")))
                    }
                    else {
                        self.file = Some(file.unwrap());
                        self.cli.push(Tokens::Path(path));
                        Ok(())
                    }
                }
                else {
                    Err(ParserErr::Expected("Path"))
                }
            },
            Tokens::Path(_) => {
                if let Tokens::As = tok {
                    self.cli.push(Tokens::As);
                    Ok(())
                }
                else {
                    Err(ParserErr::Expected("As or [finish]"))
                }
            },
            Tokens::As => {
                if let Tokens::Identifier(ident) = tok {
                    if ctx.db.resolve_ident(&ident).is_some() {
                        Err(ParserErr::ArgumentError(format!("The identifier ${} is already in use", ident)))
                    }
                    else {
                        self.ident = Some(ident.clone());
                        self.cli.push(Tokens::Identifier(ident));
                        Ok(())
                    }
                }
                else {
                    Err(ParserErr::Expected("[identifier]"))
                }
            },
            Tokens::Identifier(_) => {
                Err(ParserErr::Expected("[finish]"))
            },
            _ => unreachable!()
        }
    }
}

