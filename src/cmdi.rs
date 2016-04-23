//! Command parsers
use nom::is_digit;
use std::str::FromStr;

fn to_u32(s: &str) -> u32 {
    FromStr::from_str(s).unwrap()
}
fn is_digit_s(s: char) -> bool {
    is_digit(s as u8)
}
#[derive(Debug)]
pub enum Command<'a> {
    Load(&'a str, Option<&'a str>),
    Vol(&'a str, i32, f32, u32),
    Pos(&'a str, u32),
    StartStop(&'a str, bool)
}
/// A number.
named!(number<&str, u32>, map!(take_while_s!(is_digit_s), to_u32));
/// {identifier}: ${string}
named!(identifier<&str, &str>, complete!(chain!(tag_s!("$") ~
                                                ident: is_not_s!(" "),
                                                || {ident})));
/// LOAD {filepath} [AS {identifier}]
named!(pub load_command<&str, Command>, chain!(
    tag_s!("load ") ~
        filename: delimited!(tag_s!("\""), is_not_s!("\""), tag_s!("\"")) ~
        ident: complete!(chain!(
                tag_s!(" as ") ~
                ident: identifier,
            || {
                ident
            }
        ))?,
    || {
        Command::Load(filename, ident)
    }
));
/// VOL {identifier} [CHAN {ch}] @ {vol}[.{denom}] [FADE {secs}]
named!(pub vol_command<&str, Command>, chain!(
        tag_s!("vol ") ~
        ident: identifier ~
        chan: complete!(chain!(
                tag_s!(" chan ") ~
                ch: number,
            || {
                ch
            }
        ))? ~
        tag_s!(" @ ") ~
        vol: chain!(
                sign: tag_s!("-")? ~
                bp: number ~
                denom: complete!(chain!(
                        tag_s!(".") ~
                        denom: number,
                    || {denom}
                ))?,
            || {
                (bp as f32) * (if sign.is_some() { -1.0 } else { 1.0 }) + (if denom.is_some() {
                    (denom.unwrap() as f32) / 10.0
                } else { 0.0 })
            }
        ) ~
        fade: complete!(chain!(
                tag_s!(" fade ") ~
                secs: number,
            || {
                secs
            }
        ))?,
    || {
        Command::Vol(ident, chan.map(|x| x as i32).unwrap_or(-1), vol, fade.unwrap_or(0))
    }
));
/// POS {identifier} @ {secs}
named!(pub pos_command<&str, Command>, chain!(
    tag_s!("pos ") ~
        ident: identifier ~
        tag_s!(" @ ") ~
        secs: number,
    || {
        Command::Pos(ident, secs)
    }
));
/// {START or STOP} {identifier}
named!(pub ss_command<&str, Command>, chain!(
    ss: alt!(tag_s!("stop ") | tag_s!("start ")) ~
        ident: identifier,
    || {
        Command::StartStop(ident, ss == "start ")
    }
));
/// Parser that spits out a type of command.
named!(pub command<&str, Command>, alt!(
    complete!(load_command) |
    complete!(vol_command) |
    complete!(pos_command) |
    complete!(ss_command)
));
