use parser::{Tokens, EtokenFSM, ParserErr, SpaceRet};
use state::{ReadableContext, WritableContext, ObjectType, Database};
use rsndfile::SndFile;
use streamv2::{FileStream, FileStreamX, LiveParameters, db_lin};
use mixer::FRAMES_PER_CALLBACK;
use std::string::ToString;
use uuid::Uuid;
use commands::*;
use std::mem;

pub trait Command: Send {
    fn add(&mut self, tok: Tokens, ctx: &ReadableContext) -> Result<(), ParserErr>;
    fn back(&mut self) -> Option<Tokens>;
    fn is_complete(&self, ctx: &ReadableContext) -> Result<(), String>;
    fn line(&self) -> (Tokens, &Vec<Tokens>);
    fn execute(&mut self, ctx: &mut WritableContext);
}
pub enum CmdParserFSM {
    Idle,
    Parsing(Box<Command>),
    ParsingEtokFor(Box<Command>, EtokenFSM),
}
impl CmdParserFSM {
    pub fn new() -> Self {
        CmdParserFSM::Idle
    }
    fn char_tok(c: char) -> Option<Tokens> {
        match c {
            'l' => Some(Tokens::Load),
            'v' => Some(Tokens::Vol),
            'p' => Some(Tokens::Pos),
            's' => Some(Tokens::Start),
            'o' => Some(Tokens::Stop),
            'a' => Some(Tokens::As),
            '#' => Some(Tokens::All),
            'c' => Some(Tokens::Chan),
            '@' => Some(Tokens::At),
            'f' => Some(Tokens::Fade),
            _ => None
        }
    }
    fn cmdline_from_cmd(cmd: &Box<Command>) -> String {
        let (orig, line) = cmd.line();
        let mut ret = orig.to_string();
        for tok in line.iter() {
            ret.push_str(" ");
            ret = ret + &tok.to_string();
        }
        ret
    }
    pub fn cmdline(&self) -> String {
        match self {
            &CmdParserFSM::Idle => format!(""),
            &CmdParserFSM::Parsing(ref cmd) => CmdParserFSM::cmdline_from_cmd(cmd),
            &CmdParserFSM::ParsingEtokFor(ref cmd, ref etok) => CmdParserFSM::cmdline_from_cmd(cmd) + " " + &etok.to_string()
        }
    }
    pub fn debug_remove_me(&self, ctx: &ReadableContext) -> String {
        match self {
            &CmdParserFSM::Idle => format!("idle"),
            &CmdParserFSM::Parsing(ref cmd) => format!("parsing command (complete: {:?})", cmd.is_complete(ctx)),
            &CmdParserFSM::ParsingEtokFor(ref cmd, ref etok) => format!("parsing etok {:?} for command (complete: {:?})", etok, cmd.is_complete(ctx))
        }
    }
    pub fn back(&mut self){
        *self = match mem::replace(self, CmdParserFSM::Idle) {
            CmdParserFSM::Idle => CmdParserFSM::Idle,
            CmdParserFSM::Parsing(mut cmd) => {
                let popped = cmd.back();
                if let Some(Some(pos_etok)) = popped.map(|x| EtokenFSM::from_token(x)) {
                    if let Some(new_etok) = pos_etok.back() {
                        CmdParserFSM::ParsingEtokFor(cmd, new_etok)
                    }
                    else {
                        CmdParserFSM::Parsing(cmd)
                    }
                }
                else if cmd.line().1.len() == 0 {
                    CmdParserFSM::Idle
                }
                else {
                    CmdParserFSM::Parsing(cmd)
                }
            },
            CmdParserFSM::ParsingEtokFor(cmd, etok) => {
                if let Some(new_etok) = etok.back() {
                    CmdParserFSM::ParsingEtokFor(cmd, new_etok)
                }
                else {
                    CmdParserFSM::Parsing(cmd)
                }
            }
        }
    }
    pub fn would_enter(&mut self, ctx: &ReadableContext) -> bool {
        // TODO: not very DRY either
        match *self {
            CmdParserFSM::Idle => false,
            CmdParserFSM::Parsing(ref cmd) => {
                if let Err(_) = cmd.is_complete(ctx) {
                    false
                }
                else {
                    true
                }
            },
            CmdParserFSM::ParsingEtokFor(ref mut cmd, ref etok) => {
                // TODO: needs more DRY
                match etok.clone().finish(false) {
                    SpaceRet::Parsed(tok) => {
                        match cmd.add(tok, ctx) {
                            Ok(_) => {
                                let ret = if let Err(_) = cmd.is_complete(ctx) {
                                    false
                                }
                                else {
                                    true
                                };
                                cmd.back();
                                ret
                            }
                            Err(_) => false
                        }
                    },
                    SpaceRet::Continue(_) => {
                        false
                    },
                    SpaceRet::Incomplete(_) => {
                        false
                    },
                    SpaceRet::IntErr(_, _) => {
                        false
                    },
                    SpaceRet::FloatErr(_, _) => {
                        false
                    }
                }
            }
        }
    }
    pub fn enter(&mut self, ctx: &ReadableContext) -> Result<Option<Box<Command>>, String> {
        let mut exec = None;
        let ret = match mem::replace(self, CmdParserFSM::Idle) {
            CmdParserFSM::Idle => Ok(CmdParserFSM::Idle),
            CmdParserFSM::Parsing(mut cmd) => {
                if let Err(e) = cmd.is_complete(ctx) {
                    Err((CmdParserFSM::Parsing(cmd), e))
                }
                else {
                    exec = Some(cmd);
                    Ok(CmdParserFSM::Idle)
                }
            },
            CmdParserFSM::ParsingEtokFor(mut cmd, etok) => {
                // TODO: needs more DRY
                let oe = etok.clone();
                match etok.finish(false) {
                    SpaceRet::Parsed(tok) => {
                        match cmd.add(tok, ctx) {
                            Ok(_) => {
                                if let Err(e) = cmd.is_complete(ctx) {
                                    Err((CmdParserFSM::Parsing(cmd), e))
                                }
                                else {
                                    exec = Some(cmd);
                                    Ok(CmdParserFSM::Idle)
                                }
                            }
                            Err(e) => Err((CmdParserFSM::ParsingEtokFor(cmd, oe), Into::into(e)))
                        }
                    },
                    SpaceRet::Continue(fsm) => {
                        Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), Into::into(ParserErr::IncompleteEtoken)))
                    },
                    SpaceRet::Incomplete(fsm) => {
                        Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), Into::into(ParserErr::IncompleteEtoken)))
                    },
                    SpaceRet::IntErr(fsm, _) => {
                        Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), Into::into(ParserErr::NumError)))
                    },
                    SpaceRet::FloatErr(fsm, _) => {
                        Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), Into::into(ParserErr::NumError)))
                    }
                }
            }
        };
        match ret {
            Ok(slf) => {
                *self = slf;
                Ok(exec)
            },
            Err((slf, e)) => {
                *self = slf;
                Err(e)
            }
        }
    }
    pub fn addc(&mut self, c: char, ctx: &ReadableContext) -> Result<(), ParserErr> {
        let ret = match mem::replace(self, CmdParserFSM::Idle) {
            CmdParserFSM::Idle => {
                match CmdParserFSM::char_tok(c) {
                    Some(Tokens::Load) => {
                        Ok(CmdParserFSM::Parsing(Box::new(LoadCommand::new())))
                    },
                    Some(Tokens::Vol) => {
                        Ok(CmdParserFSM::Parsing(Box::new(VolCommand::new())))
                    },
                    Some(Tokens::Pos) => {
                        Ok(CmdParserFSM::Parsing(Box::new(PosCommand::new())))
                    },
                    Some(Tokens::Stop) => {
                        Ok(CmdParserFSM::Parsing(Box::new(StopCommand::new())))
                    },
                    Some(Tokens::Start) => {
                        Ok(CmdParserFSM::Parsing(Box::new(StartCommand::new())))
                    },
                    _ => Err((CmdParserFSM::Idle, ParserErr::Expected("Load, Vol, Stop, Start or Pos")))
                }
            },
            CmdParserFSM::Parsing(mut cmd) => {
                if let Some(tok) = CmdParserFSM::char_tok(c) {
                    match cmd.add(tok, ctx) {
                        Ok(_) => Ok(CmdParserFSM::Parsing(cmd)),
                        Err(e) => Err((CmdParserFSM::Parsing(cmd), e))
                    }
                }
                else if let Ok(etok) = EtokenFSM::new().addc(c) {
                    Ok(CmdParserFSM::ParsingEtokFor(cmd, etok))
                }
                else {
                    Err((CmdParserFSM::Parsing(cmd), ParserErr::InvalidToken))
                }
            },
            CmdParserFSM::ParsingEtokFor(mut cmd, etok) => {
                if c == ' ' {
                    let oe = etok.clone();
                    match etok.finish(true) {
                        SpaceRet::Parsed(tok) => {
                            match cmd.add(tok, ctx) {
                                Ok(_) => Ok(CmdParserFSM::Parsing(cmd)),
                                Err(e) => Err((CmdParserFSM::ParsingEtokFor(cmd, oe), e))
                            }
                        },
                        SpaceRet::Continue(fsm) => {
                            Ok(CmdParserFSM::ParsingEtokFor(cmd, fsm))
                        },
                        SpaceRet::Incomplete(fsm) => {
                            Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), ParserErr::IncompleteEtoken))
                        },
                        SpaceRet::IntErr(fsm, _) => {
                            Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), ParserErr::NumError))
                        },
                        SpaceRet::FloatErr(fsm, _) => {
                            Err((CmdParserFSM::ParsingEtokFor(cmd, fsm), ParserErr::NumError))
                        }
                    }
                }
                else {
                    match etok.addc(c) {
                        Ok(fsm) => Ok(CmdParserFSM::ParsingEtokFor(cmd, fsm)),
                        Err((opt, err)) => Err((CmdParserFSM::ParsingEtokFor(cmd, opt.unwrap()), err))
                    }
                }
            }
        };
        match ret {
            Ok(slf) => {
                *self = slf;
                Ok(())
            },
            Err((slf, e)) => {
                *self = slf;
                Err(e)
            }
        }
    }
}
