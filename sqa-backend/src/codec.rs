use std::net::SocketAddr;
use tokio_core::net::UdpCodec;
use rosc::{decoder, encoder, OscMessage, OscPacket, OscType};
use errors::*;
use mixer::MixerConf;
use errors::BackendErrorKind::*;
use actions::{ActionParameters, OpaqueAction};
use std::collections::HashMap;
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
    #[oscpath = "/actions/{typ}/new"]
    CreateAction { #[subst] typ: String },
    #[oscpath = "/actions/{typ}/new/withparams"]
    CreateActionWithParams { #[subst] typ: String, #[ser] params: ActionParameters },
    #[oscpath = "/actionlist"]
    ActionList,
    #[oscpath = "/action/{uuid}"]
    ActionInfo { #[subst] uuid: Uuid },
    #[oscpath = "/action/{uuid}/update"]
    UpdateActionParams { #[subst] uuid: Uuid, #[ser] params: ActionParameters },
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
    #[oscpath = "/error/deserfail"]
    DeserFailed { #[verbatim = "string"] err: String },

    #[oscpath = "/reply/actions/create"]
    ActionCreated { #[ser] res: Result<Uuid, String> },
    #[oscpath = "/reply/action/{uuid}"]
    ActionInfoRetrieved { #[subst] uuid: Uuid, #[ser] res: Result<OpaqueAction, String> },
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
    #[oscpath = "/reply/actionlist"]
    ReplyActionList { #[ser] list: HashMap<Uuid, OpaqueAction> },

    #[oscpath = "/update/action/{uuid}"]
    UpdateActionInfo { #[subst] uuid: Uuid, #[ser] data: OpaqueAction },
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
