use std::net::SocketAddr;
use serde::{Deserialize, Serialize};
use serde_json::{self, Deserializer, Serializer};
use tokio_core::net::UdpCodec;
use rosc::{decoder, encoder, OscMessage, OscPacket, OscType};
use errors::*;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Command {
    /// /ping -> /pong
    Ping,
    /// /pong
    Pong,
    /// /create {type} -> /reply/create UUID
    CreateAction { typ: String },
    /// /action/{uuid} -> /reply/action/{uuid} {parameters}
    ActionParams { uuid: Uuid },
    /// /action/{uuid}/update {parameters} -> /reply/action/{uuid}/update
    UpdateActionParams { uuid: Uuid, params: String },
    /// /action/{uuid}/delete -> /reply/action/{uuid}/delete
    DeleteAction { uuid: Uuid },
    /// /action/{uuid}/{method} {???} -> ???
    ActionMethod { uuid: Uuid, path: Vec<String>, args: Vec<OscType> },
    /// /action/{uuid}/verify -> Vec<ParameterError>
    VerifyAction { uuid: Uuid },
    /// /action/{uuid}/load -> Result
    LoadAction { uuid: Uuid },
    /// /action/{uuid}/execute -> Result
    ExecuteAction { uuid: Uuid }
}
#[derive(Debug)]
pub struct RecvMessage {
    pub addr: SocketAddr,
    pub pkt: BackendResult<(String, Command)>
}
#[derive(Debug)]
pub struct SendMessage {
    pub addr: SocketAddr,
    pub pkt: OscMessage
}
pub trait SendMessageExt {
    fn msg_to(&self, c: OscMessage) -> SendMessage;
}
impl SendMessageExt for SocketAddr {
    fn msg_to(&self, c: OscMessage) -> SendMessage {
        let mut addr = self.clone();
        addr.set_port(53001);
        SendMessage {
            addr: addr,
            pkt: c
        }
    }
}
fn parse_osc_message(addr: &str, args: Option<Vec<OscType>>) -> BackendResult<Command> {
    let path: Vec<&str> = (&addr).split("/").collect();
    let mut args = if let Some(a) = args { a } else { vec![] };
    if path.len() < 2 {
        bail!(BackendErrorKind::MalformedOSCPath);
    }
    match &path[1..] {
        &["ping"] => Ok(Command::Ping),
        &["create"] => {
            if args.len() != 1 {
                bail!(BackendErrorKind::MalformedOSCPath);
            }
            if let Some(x) = args.remove(0).string() {
                Ok(Command::CreateAction { typ: x })
            }
            else {
                bail!(BackendErrorKind::MalformedOSCPath);
            }
        },
        &["action", uuid] => {
            let uuid = Uuid::parse_str(uuid)?;
            Ok(Command::ActionParams { uuid: uuid })
        },
        &["action", uuid, cmd, ref a..] => {
            let uuid = Uuid::parse_str(uuid)?;
            match cmd {
                "update" => {
                    if args.len() != 1 {
                        bail!(BackendErrorKind::MalformedOSCPath);
                    }
                    if let Some(x) = args.remove(0).string() {
                        Ok(Command::UpdateActionParams { uuid: uuid, params: x })
                    }
                    else {
                        bail!(BackendErrorKind::MalformedOSCPath);
                    }
                },
                "delete" => {
                    Ok(Command::DeleteAction { uuid: uuid })
                },
                "verify" => {
                    Ok(Command::VerifyAction { uuid: uuid })
                },
                "load" => {
                    Ok(Command::LoadAction { uuid: uuid })
                },
                "execute" => {
                    Ok(Command::ExecuteAction { uuid: uuid })
                },
                _ => {
                    // hooray for iterators!
                    let path = [cmd].iter().chain(a.iter()).map(|s| s.to_string()).collect();
                    Ok(Command::ActionMethod { uuid: uuid, path: path, args: args })
                }
            }
        },
        _ => Err(BackendErrorKind::UnknownOSCPath.into())
    }
}

pub struct SqaWireCodec;
impl UdpCodec for SqaWireCodec {
    type In = RecvMessage;
    type Out = SendMessage;
    fn decode(&mut self, src: &SocketAddr, buf: &[u8]) -> ::std::io::Result<Self::In> {
        let pkt = match decoder::decode(buf) {
            Ok(pkt) => {
                match pkt {
                    OscPacket::Message(m) => {
                        let OscMessage { addr, args } = m;
                        match parse_osc_message(&addr, args) {
                            Ok(r) => Ok((addr, r)),
                            Err(e) => Err(e)
                        }
                    },
                    _ => Err(BackendErrorKind::UnsupportedOSCBundle.into())
                }
            },
            Err(e) => Err(e.into())
        };
        Ok(RecvMessage {
            addr: src.clone(),
            pkt: pkt
        })
    }
    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> SocketAddr {
        let SendMessage { pkt, addr } = msg;
        if let Ok(b) = encoder::encode(&OscPacket::Message(pkt)) {
            ::std::mem::replace(buf, b);
        }
        addr
    }
}
