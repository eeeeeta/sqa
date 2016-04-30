//! Command-line parsing functions.
use std::str::FromStr;
use std::fmt;
/// A set of valid tokens on a command line.
#[derive(Debug, Clone)]
pub enum Tokens {
    /// A file path.
    Path(String),
    /// A named identifier (`$foo`).
    Identifier(String),
    /// An integer.
    Num(u16),
    /// A floating-point number.
    Float(f32),
    /// `LOAD` command.
    Load,
    /// `AS` qualifier.
    As,
    /// `VOL` command.
    Vol,
    /// `CHAN` qualifier.
    Chan,
    /// `@` symbol.
    At,
    /// `POS` command.
    Pos,
    /// `START` command.
    Start,
    /// `STOP` command.
    Stop,
    /// `ALL` qualifier.
    All
}
impl fmt::Display for Tokens {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Tokens::Path(ref st) => write!(f, "\"{}\"", st),
            &Tokens::Identifier(ref id) => write!(f, "${}", id),
            &Tokens::Num(n) => write!(f, "{}", n),
            &Tokens::Float(n) => write!(f, "{}", n),
            &Tokens::Load => write!(f, "Load"),
            &Tokens::As => write!(f, "As"),
            &Tokens::Vol => write!(f, "Vol"),
            &Tokens::Chan => write!(f, "Chan"),
            &Tokens::At => write!(f, "@"),
            &Tokens::Pos => write!(f, "Pos"),
            &Tokens::Start => write!(f, "Start"),
            &Tokens::Stop => write!(f, "Stop"),
            &Tokens::All => write!(f, "All")
        }
    }
}
/// Finite-state machine for extended tokens.
///
/// An *extended token* (etoken) is a token which is comprised from more than one
/// user input, like a path or identifier (as opposed to a simple keypress to insert a
/// non-extended token).
#[derive(Debug, Clone)]
pub enum EtokenFSM {
    /// Idle.
    Idle,
    /// The start of a filepath `"`.
    FilePathStart,
    /// A filepath fragment `"blah`.
    FilePathFragment(String),
    /// A complete filepath `"blah"`.
    FilePathComplete(String),
    /// The start of an identifier `$`.
    IdentifierStart,
    /// An identifier fragment `$foo`.
    IdentifierFragment(String),
    /// A number `42`.
    Number(String),
    /// A number with a decimal point `42.`.
    NumberDot(String),
    /// A floating-point number `42.2`.
    NumberDec(String, String)
}
use self::EtokenFSM::*;
impl fmt::Display for EtokenFSM {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Idle => write!(f, ""),
            &FilePathStart => write!(f, "\""),
            &FilePathFragment(ref frag) => write!(f, "\"{}", frag),
            &FilePathComplete(ref frag) => write!(f, "\"{}\"", frag),
            &IdentifierStart => write!(f, "$"),
            &IdentifierFragment(ref frag) => write!(f, "${}", frag),
            &Number(ref frag) => write!(f, "{}", frag),
            &NumberDot(ref frag) => write!(f, "{}.", frag),
            &NumberDec(ref orig, ref frag) => write!(f, "{}.{}", orig, frag)
        }
    }
}
#[derive(Debug)]
pub enum ParserErr {
    Expected(&'static str),
    ArgumentError(String),
    InvalidToken,
    IncompleteEtoken,
    NumError
}
pub enum SpaceRet {
    Parsed(Tokens),
    Continue(EtokenFSM),
    Incomplete(EtokenFSM),
    IntErr(EtokenFSM, ::std::num::ParseIntError),
    FloatErr(EtokenFSM, ::std::num::ParseFloatError)
}
pub fn string_from_char(c: char) -> String {
    let mut st = String::new();
    st.push(c);
    st
}
impl EtokenFSM {
    pub fn new() -> Self {
        Idle
    }
    pub fn finish(self, spc: bool) -> SpaceRet {
        match self {
            Idle => SpaceRet::Incomplete(self),
            FilePathStart => SpaceRet::Incomplete(self),
            FilePathFragment(mut frag) => {
                if spc {
                    frag.push(' ');
                    SpaceRet::Continue(FilePathFragment(frag))
                }
                else {
                    SpaceRet::Incomplete(FilePathFragment(frag))
                }
            },
            FilePathComplete(frag) => SpaceRet::Parsed(Tokens::Path(frag)),
            IdentifierStart => SpaceRet::Incomplete(self),
            IdentifierFragment(frag) => SpaceRet::Parsed(Tokens::Identifier(frag)),
            Number(frag) => {
                match u16::from_str(&frag) {
                    Ok(i) => SpaceRet::Parsed(Tokens::Num(i)),
                    Err(e) => SpaceRet::IntErr(Number(frag), e)
                }
            },
            NumberDot(frag) => SpaceRet::Incomplete(NumberDot(frag)),
            NumberDec(orig, frag) => {
                match f32::from_str(&((orig.clone() + ".") + &frag)) {
                    Ok(f) => SpaceRet::Parsed(Tokens::Float(f)),
                    Err(e) => SpaceRet::FloatErr(NumberDec(orig, frag), e)
                }
            }
        }
    }
    pub fn back(self) -> Option<Self> {
        match self {
            Idle => None,
            FilePathStart => None,
            FilePathFragment(mut frag) => {
                Some(if frag.pop().is_some() && frag.len() > 0 {
                    FilePathFragment(frag)
                }
                     else {
                         FilePathStart
                     })
            },
            FilePathComplete(frag) => Some(FilePathFragment(frag)),
            IdentifierStart => None,
            IdentifierFragment(mut frag) => {
                Some(if frag.pop().is_some() && frag.len() > 0 {
                    IdentifierFragment(frag)
                }
                     else {
                         IdentifierStart
                     })
            },
            Number(mut frag) => {
                if frag.pop().is_some() && frag.len() > 0 {
                    Some(Number(frag))
                }
                else {
                    None
                }
            },
            NumberDot(frag) => Some(Number(frag)),
            NumberDec(orig, mut frag) => {
                Some(if frag.pop().is_some() && frag.len() > 0 {
                    NumberDec(orig, frag)
                }
                else {
                    NumberDot(orig)
                })
            }
        }
    }
    // FIXME: returning an Option makes no sense
    pub fn addc(self, c: char) -> Result<Self, (Option<Self>, ParserErr)> {
        match self {
            Idle => {
                match c {
                    '"' => Ok(FilePathStart),
                    '$' => Ok(IdentifierStart),
                    num @ '0' ... '9' => Ok(Number(string_from_char(num))),
                    _ => Err((Some(Idle), ParserErr::Expected("'\"', '$' or 0..9")))
                }
            },
            FilePathStart => Ok(FilePathFragment(string_from_char(c))),
            FilePathFragment(mut frag) => {
                match c {
                    '"' => Ok(FilePathComplete(frag)),
                    _ => {
                        frag.push(c);
                        Ok(FilePathFragment(frag))
                    }
                }
            },
            FilePathComplete(frag) => Err((Some(FilePathComplete(frag)), ParserErr::Expected("[finish]"))),
            IdentifierStart => Ok(IdentifierFragment(string_from_char(c))),
            IdentifierFragment(mut frag) => {
                frag.push(c);
                Ok(IdentifierFragment(frag))
            },
            Number(mut frag) => {
                match c {
                    num @ '0' ... '9' => {
                        frag.push(num);
                        Ok(Number(frag))
                    },
                    '.' => Ok(NumberDot(frag)),
                    _ => Err((Some(Number(frag)), ParserErr::Expected("0..9 or '.'")))
                }
            },
            NumberDot(frag) => {
                match c {
                    num @ '0' ... '9' => Ok(NumberDec(frag, string_from_char(num))),
                    _ => Err((Some(NumberDot(frag)), ParserErr::Expected("0..9")))
                }
            },
            NumberDec(orig, mut frag) => {
                match c {
                    num @ '0' ... '9' => {
                        frag.push(num);
                        Ok(NumberDec(orig, frag))
                    },
                    _ => Err((Some(NumberDec(orig, frag)), ParserErr::Expected("0..9")))
                }
            }
        }
    }
}
