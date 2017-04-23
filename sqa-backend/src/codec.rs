use std::net::SocketAddr;
use tokio_core::net::UdpCodec;
use rosc::{decoder, encoder, OscMessage, OscPacket, OscType};
use errors::*;
use mixer::MixerConf;
use errors::BackendErrorKind::*;
use serde_json;
use uuid::Uuid;

type OscResult<T> = BackendResult<T>;

#[derive(OscSerde, Debug, Clone)]
pub enum Command {
    #[oscpath = "/ping"]
    Ping,
    #[oscpath = "/version"]
    Version,
    #[oscpath = "/subscribe"]
    Subscribe,
    #[oscpath = "/action/create"]
    CreateAction { #[verbatim = "string"] typ: String },
    #[oscpath = "/action/{uuid}"]
    ActionInfo { #[subst] uuid: Uuid },
    #[oscpath = "/action/{uuid}/update"]
    UpdateActionParams { #[subst] uuid: Uuid, #[verbatim = "string"] params: String },
    #[oscpath = "/action/{uuid}/delete"]
    DeleteAction { #[subst] uuid: Uuid },
/*
    /// /action/{uuid}/{method} {???} -> ???
    ActionMethod { uuid: Uuid, path: Vec<String>, args: Vec<OscType> },
*/
    #[oscpath = "/action/{uuid}/verify"]
    VerifyAction { #[subst] uuid: Uuid },
    #[oscpath = "/action/{uuid}/load"]
    LoadAction { #[subst] uuid: Uuid },
    #[oscpath = "/action/{uuid}/execute"]
    ExecuteAction { #[subst] uuid: Uuid },
    #[oscpath = "/mixer/config"]
    GetMixerConf,
    #[oscpath = "/mixer/config/set"]
    SetMixerConf { #[ser] conf: MixerConf }
}
impl Into<OscMessage> for Command {
    fn into(self) -> OscMessage {
        self.to_osc()
            .expect("No impl generated, check codec.rs")
    }
}
#[derive(OscSerde, Debug, Clone)]
pub enum Reply {
    #[oscpath = "/pong"]
    Pong,
    #[oscpath = "/reply/version"]
    ServerVersion { #[verbatim = "string"] ver: String },
    #[oscpath = "/reply/subscribe"]
    Subscribed,

    #[oscpath = "/reply/action/create"]
    ActionCreated { #[ser] res: Result<Uuid, String> },
    #[oscpath = "/reply/action/{uuid}"]
    ActionInfoRetrieved { #[subst] uuid: Uuid, #[ser] res: Result<serde_json::Value, String> },
    #[oscpath = "/reply/action/{uuid}/update"]
    ActionParamsUpdated { #[subst] uuid: Uuid, #[ser] res: Result<(), String> },
    #[oscpath = "/reply/action/{uuid}/delete"]
    ActionDeleted { #[subst] uuid: Uuid, #[ser] deleted: bool },
    #[oscpath = "/reply/action/{uuid}/load"]
    ActionLoaded { #[subst] uuid: Uuid, #[ser] res: Result<(), String> },
    #[oscpath = "/reply/action/{uuid}/execute"]
    ActionExecuted { #[subst] uuid: Uuid, #[ser] res: Result<(), String> },
    #[oscpath = "/reply/mixer/config"]
    MixerConfSet { #[ser] res: Result<(), String> },

    #[oscpath = "/update/action/create"]
    UpdateActionCreated { #[ser] uuid: Uuid },
    #[oscpath = "/update/action/{uuid}"]
    UpdateActionInfo { #[subst] uuid: Uuid, #[ser] data: serde_json::Value },
    #[oscpath = "/update/action/{uuid}/delete"]
    UpdateActionDeleted { #[subst] uuid: Uuid },
    #[oscpath = "/update/mixer/config"]
    UpdateMixerConf { #[ser] conf: MixerConf },
}
impl Into<OscMessage> for Reply {
    fn into(self) -> OscMessage {
        self.to_osc()
            .expect("No impl generated, check codec.rs")
    }
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
/*pub fn parse_osc_reply(addr: &str, args: Option<Vec<OscType>>) -> BackendResult<Reply> {
    use self::Reply::*;

    let path: Vec<&str> = (&addr).split("/").collect();
    let mut args = if let Some(a) = args { a } else { vec![] };
    if path.len() < 2 {
        bail!("Blank OSC path.");
    }
    match &path[1..] {
        &["pong"] => Ok(Pong),
        &["reply", "subscribe"] => Ok(Subscribed),
        &["reply", "version"] => {
            if args.len() != 1 {
                bail!(OSCWrongArgs(args.len(), 1));
            }
            if let Some(ver) = args.remove(0).string() {
                Ok(ServerVersion { ver })
            }
            else {
                bail!(OSCWrongType(0, "string"));
            }
        },
        _ => Err(BackendErrorKind::UnknownOSCPath.into())
    }
}
pub fn parse_osc_message(addr: &str, args: Option<Vec<OscType>>) -> BackendResult<Command> {
    let path: Vec<&str> = (&addr).split("/").collect();
    let mut args = if let Some(a) = args { a } else { vec![] };
    if path.len() < 2 {
        bail!("Blank OSC path.");
    }
    match &path[1..] {
        &["ping"] => Ok(Command::Ping),
        &["subscribe"] => Ok(Command::Subscribe),
        &["version"] => Ok(Command::Version),
        &["create"] => {
            if args.len() != 1 {
                bail!(OSCWrongArgs(args.len(), 1));
            }
            if let Some(x) = args.remove(0).string() {
                Ok(Command::CreateAction { typ: x })
            }
            else {
                bail!(OSCWrongType(0, "string"));
            }
        },
        &["action", uuid] => {
            let uuid = Uuid::parse_str(uuid)?;
            Ok(Command::ActionInfo { uuid: uuid })
        },
        &["mixer", "config"] => {
            Ok(Command::GetMixerConf)
        },
        &["mixer", "config", "set"] => {
            if args.len() != 1 {
                bail!(OSCWrongArgs(args.len(), 1));
            }
            if let Some(x) = args.remove(0).string() {
                let conf = serde_json::from_str(&x)?;
                Ok(Command::SetMixerConf { conf: conf })
            }
            else {
                bail!(OSCWrongType(0, "string"));
            }

        },
        &["action", uuid, cmd, ref a..] => {
            let uuid = Uuid::parse_str(uuid)?;
            match cmd {
                "update" => {
                    if args.len() != 1 {
                        bail!(OSCWrongArgs(args.len(), 1));
                    }
                    if let Some(x) = args.remove(0).string() {
                        Ok(Command::UpdateActionParams { uuid: uuid, params: x })
                    }
                    else {
                        bail!(OSCWrongType(0, "string"));
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
}*/
pub struct SqaClientCodec {
    addr: SocketAddr
}
impl SqaClientCodec {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }
}
impl UdpCodec for SqaClientCodec {
    type In = BackendResult<Reply>;
    type Out = Command;
    fn decode(&mut self, src: &SocketAddr, buf: &[u8]) -> ::std::io::Result<Self::In> {
        if self.addr != *src {
            return Ok(Err("Received a message from another server.".into()));
        }
        let pkt = match decoder::decode(buf) {
            Ok(pkt) => {
                match pkt {
                    OscPacket::Message(m) => {
                        let OscMessage { addr, args } = m;
                        match Reply::from_osc(&addr, args) {
                            Ok(r) => Ok(r),
                            Err(e) => Err(e)
                        }
                    },
                    _ => Err(BackendErrorKind::UnsupportedOSCBundle.into())
                }
            },
            Err(e) => Err(e.into())
        };
        Ok(pkt)
    }
    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> SocketAddr {
        if let Ok(b) = encoder::encode(&OscPacket::Message(msg.into())) {
            ::std::mem::replace(buf, b);
        }
        self.addr
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
                        match Command::from_osc(&addr, args) {
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
