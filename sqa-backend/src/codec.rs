use std::net::SocketAddr;
use serde::{Deserialize, Serialize};
use serde_json::{self, Deserializer, Serializer};
use tokio_core::net::UdpCodec;
use rosc::{decoder, encoder, OscMessage, OscPacket};
use errors::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Command {
    /// /ping
    Ping,
    /// /pong
    Pong,
}
#[derive(Debug)]
pub struct RecvMessage {
    pub addr: SocketAddr,
    pub pkt: BackendResult<Command>
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
fn parse_osc_message(m: OscMessage) -> BackendResult<Command> {
    let OscMessage { addr, args } = m;
    let path: Vec<&str> = (&addr).split("/").collect();
    if path.len() < 2 {
        bail!(BackendErrorKind::MalformedOSCPath);
    }
    match path[1] {
        "ping" => Ok(Command::Ping),
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
                    OscPacket::Message(m) => parse_osc_message(m),
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
