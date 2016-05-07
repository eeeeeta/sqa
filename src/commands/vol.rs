use parser::{Tokens, ParserErr};
use command::Command;
use streamv2::{FileStreamX, db_lin};
use uuid::Uuid;
use mixer::FRAMES_PER_CALLBACK;
use state::{ReadableContext, WritableContext, ObjectType, Database};


pub struct VolCommand {
    cli: Vec<Tokens>,
    ident: Option<Uuid>,
    chan: isize,
    target: Option<i16>,
    fade: Option<f32>
}
impl VolCommand {
    pub fn new() -> Self {
        VolCommand {
            cli: Vec::new(),
            ident: None,
            chan: -1,
            target: None,
            fade: None
        }
    }
}
impl Command for VolCommand {
    fn execute(&mut self, ctx: &mut WritableContext) {
        let (ident, chan, target) = (self.ident.take().unwrap(), self.chan, db_lin(self.target.take().unwrap() as f32));
        let mut fsx = ctx.db.control_filestream(&ident).unwrap();
        for (i, ch) in fsx.iter_mut().enumerate() {
            if chan == i as isize || chan == -1 {
                if self.fade.is_some() {
                    let lp = ch.lp();
                    let end = lp.pos + (self.fade.unwrap() * 44_100 as f32) as usize;
                    ch.set_fader(Box::new(move |pos, vol, _| {
                        let fade_left = *vol - target;
                        if fade_left == 0.0 { return false };
                        let units_left = (end - pos) / FRAMES_PER_CALLBACK;
                        if units_left == 0 {
                            *vol = target;
                            true
                        }
                        else {
                            *vol = *vol - (fade_left / units_left as f32);
                            true
                        }
                    }));
                }
                else {
                    ch.set_vol(target);
                }
            }
        }
    }
    fn is_complete(&self, ctx: &ReadableContext) -> Result<(), String> {
        if self.ident.is_none() { Err(format!("No identifier to fade.")) }
        else if self.target.is_none() { Err(format!("No volume to fade to.")) }
        else {
            let id = self.ident.as_ref().unwrap();
            if let Some(&ObjectType::FileStream(_, _)) = ctx.db.type_of(id) {
                if self.chan != -1 && ctx.db.get(id).unwrap().others.as_ref().unwrap().len() <= self.chan as usize {
                    Err(format!("The identifier given does not have a channel numbered {}.", self.chan))
                }
                else {
                    Ok(())
                }
            }
            else {
                Err(format!("The identifier given does not exist or is of an invalid type."))
            }
        }
    }
    fn line(&self) -> (Tokens, &Vec<Tokens>) {
        (Tokens::Vol, &self.cli)
    }
    fn back(&mut self) -> Option<Tokens> {
        {
            let ld = Tokens::Vol; // to appease borrowck
            let mut biter = self.cli.iter();
            let last = biter.next_back().unwrap_or(&ld);
            match last {
                &Tokens::Vol => {},
                &Tokens::Fade => {},
                &Tokens::At => {},
                &Tokens::Chan => {},
                &Tokens::Identifier(_) => { self.ident = None },
                &Tokens::Num(_) | &Tokens::NegNum(_) => {
                    match biter.next_back().unwrap() {
                        &Tokens::At => { self.target = None },
                        &Tokens::Chan => { self.chan = -1 },
                        &Tokens::Fade => { self.fade = None },
                        _ => unreachable!()
                    }
                },
                &Tokens::Float(_) => { self.fade = None },
                _ => unreachable!()
            }
        }
        self.cli.pop()
    }
    fn add(&mut self, tok: Tokens, ctx: &ReadableContext) -> Result<(), ParserErr> {
        let last = self.cli.iter().next_back().unwrap_or(&Tokens::Vol).clone();
        match last {
            Tokens::Vol => {
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
                match tok {
                    Tokens::At => {
                        self.cli.push(Tokens::At);
                        Ok(())
                    },
                    Tokens::Chan => {
                        self.cli.push(Tokens::Chan);
                        Ok(())
                    },
                    _ => Err(ParserErr::Expected("@ or Chan"))
                }
            },
            Tokens::Chan => {
                if let Tokens::Num(n) = tok {
                    self.chan = n as isize;
                    self.cli.push(Tokens::Num(n));
                    Ok(())
                }
                else {
                    Err(ParserErr::Expected("Num"))
                }
            },
            Tokens::At => {
                match tok {
                    Tokens::Num(n) => {
                        self.target = Some(n as i16);
                        self.cli.push(Tokens::Num(n));
                        Ok(())
                    },
                    Tokens::NegNum(n) => {
                        self.target = Some(n);
                        self.cli.push(Tokens::NegNum(n));
                        Ok(())
                    },
                    _ => Err(ParserErr::Expected("Num or NegNum"))
                }
            },
            Tokens::Num(_) | Tokens::NegNum(_) => {
                match {
                    let mut i = self.cli.iter();
                    i.next_back();
                    i.next_back().unwrap().clone()
                } {
                    Tokens::At => {
                        if let Tokens::Fade = tok {
                            self.cli.push(Tokens::Fade);
                            Ok(())
                        }
                        else {
                            Err(ParserErr::Expected("Fade or [finish]"))
                        }
                    },
                    Tokens::Chan => {
                        if let Tokens::At = tok {
                            self.cli.push(Tokens::At);
                            Ok(())
                        }
                        else {
                            Err(ParserErr::Expected("@"))
                        }
                    },
                    Tokens::Fade => Err(ParserErr::Expected("[finish]")),
                    _ => unreachable!()
                }
            },
            Tokens::Fade => {
                match tok {
                    Tokens::Num(n) => {
                        self.fade = Some(n as f32);
                        self.cli.push(Tokens::Num(n));
                        Ok(())
                    },
                    Tokens::Float(f) => {
                        self.fade = Some(f);
                        self.cli.push(Tokens::Float(f));
                        Ok(())
                    }
                    _ => Err(ParserErr::Expected("Num or Float"))
                }
            },
            Tokens::Float(_) => Err(ParserErr::Expected("[finish]")),
            _ => unreachable!()
        }
    }
}
