use parser::{Tokens, ParserErr};
use command::Command;
use streamv2::FileStreamX;
use uuid::Uuid;
use state::{ReadableContext, WritableContext, ObjectType, Database};


pub struct PosCommand {
    cli: Vec<Tokens>,
    ident: Option<Uuid>,
    pos: Option<u16>
}
impl PosCommand {
    pub fn new() -> Self {
        PosCommand {
            cli: Vec::new(),
            ident: None,
            pos: None
        }
    }
}
impl Command for PosCommand {
    fn line(&self) -> (Tokens, &Vec<Tokens>) {
        (Tokens::Pos, &self.cli)
    }
    fn execute(&mut self, ctx: &mut WritableContext) {
        let (ident, pos) = (self.ident.take().unwrap(), self.pos.take().unwrap());
        ctx.db.control_filestream(&ident).unwrap()[0].reset_pos(pos as u64);
    }
    fn is_complete(&self, ctx: &ReadableContext) -> Result<(), String> {
        if self.ident.is_none() {
            Err(format!("No identifier."))
        }
        else if self.pos.is_none() {
            Err(format!("No target position."))
        }
        else if let Some(&ObjectType::FileStream(_, _)) = ctx.db.type_of(self.ident.as_ref().unwrap()) {
            Ok(())
        }
        /* FIXME
        else if ctx.idents.get(self.ident.as_ref().unwrap()).as_ref().unwrap()[0].lp().end < (44_100 * self.pos.unwrap()) as u64 {
            Err(format!("The target position is greater than the endpoint of the selected identifier."))
        }*/
        else {
            Err(format!("The targeted identifier does not exist or is of an invalid type."))
        }
    }
    fn back(&mut self) -> Option<Tokens> {
        {
            let ld = Tokens::Pos;
            let mut biter = self.cli.iter();
            let last = biter.next_back().unwrap_or(&ld);
            match last {
                &Tokens::Pos => {},
                &Tokens::At => {},
                &Tokens::Identifier(_) => { self.ident = None },
                &Tokens::Num(_) => { self.pos = None },
                _ => unreachable!()
            }
        }
        self.cli.pop()
    }
    fn add(&mut self, tok: Tokens, ctx: &ReadableContext) -> Result<(), ParserErr> {
        let last = self.cli.iter().next_back().unwrap_or(&Tokens::Pos).clone();
        match last {
            Tokens::Pos => {
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
                    Err(ParserErr::Expected("[identifier]"))
                }
            },
            Tokens::Identifier(_) => {
                if let Tokens::At = tok {
                    self.cli.push(Tokens::At);
                    Ok(())
                }
                else {
                    Err(ParserErr::Expected("@"))
                }
            },
            Tokens::At => {
                if let Tokens::Num(n) = tok {
                    self.pos = Some(n);
                    self.cli.push(Tokens::Num(n));
                    Ok(())
                }
                else {
                    Err(ParserErr::Expected("Num"))
                }
            },
            Tokens::Num(_) => Err(ParserErr::Expected("[finish]")),
            _ => unreachable!()
        }
    }
}
