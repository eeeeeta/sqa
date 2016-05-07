use parser::{Tokens, ParserErr};
use command::Command;
use streamv2::FileStreamX;
use uuid::Uuid;
use state::{ReadableContext, WritableContext, ObjectType, Database};

pub struct StopCommand {
    cli: Vec<Tokens>,
    ident: Option<Uuid>
}
impl StopCommand {
    pub fn new() -> Self {
        StopCommand {
            cli: Vec::new(),
            ident: None
        }
    }
}
impl Command for StopCommand {
    fn line(&self) -> (Tokens, &Vec<Tokens>) {
        (Tokens::Stop, &self.cli)
    }
    fn execute(&mut self, ctx: &mut WritableContext) {
        for ch in ctx.db.iter_mut_type(ObjectType::FileStream(String::new(), 0), self.ident.as_ref()) {
            ch.controller.as_mut().unwrap().downcast_mut::<FileStreamX>().unwrap().pause();
        }
    }
    fn is_complete(&self, ctx: &ReadableContext) -> Result<(), String> {
        if self.ident.is_some() {
            if let Some(&ObjectType::FileStream(_, _)) = ctx.db.type_of(self.ident.as_ref().unwrap()) {
                Ok(())
            }
            else {
                Err(format!("The targeted identifier does not exist or is of an invalid type."))
            }
        }
        else {
            Ok(())
        }
    }
    fn back(&mut self) -> Option<Tokens> {
        {
            let ld = Tokens::Stop;
            let mut biter = self.cli.iter();
            let last = biter.next_back().unwrap_or(&ld);
            match last {
                &Tokens::Stop => {},
                &Tokens::Identifier(_) => { self.ident = None },
                _ => unreachable!()
            }
        }
        self.cli.pop()
    }
    fn add(&mut self, tok: Tokens, ctx: &ReadableContext) -> Result<(), ParserErr> {
        let last = self.cli.iter().next_back().unwrap_or(&Tokens::Stop).clone();
        match last {
            Tokens::Stop => {
                if let Tokens::Identifier(id) = tok {
                    if let Some((uu, ty)) = ctx.db.resolve_ident(&id) {
                        if let ObjectType::FileStream(_, _) = ty {
                            self.ident = Some(uu.clone());
                            self.cli.push(Tokens::Identifier(id));
                            Ok(())
                        }
                        else {
                            Err(ParserErr::ArgumentError(format!("The identifier given does not refer to a valid audio stream.")))
                        }
                    }
                    else {
                        Err(ParserErr::ArgumentError(format!("The identifier given does not exist.")))
                    }
                }
                else {
                    Err(ParserErr::Expected("[identifier] or [finish]"))
                }
            },
            Tokens::Identifier(_) => Err(ParserErr::Expected("[finish]")),
            _ => unreachable!()
        }
    }
}

