use std::net::SocketAddr;
use serde::{Deserialize, Serialize};
use rmp_serde::{self, Deserializer, Serializer};
use tokio_core::net::UdpCodec;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Command {
    Subscribe,
    HelloClient { version: String },
    Ping,
    Pong,
    DeserializationFailed,
    MessageSkipped
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Packet {
    pub id: u32,
    pub reply_to: u32,
    pub cmd: Command
}
#[derive(Debug)]
pub struct RecvMessage {
    pub addr: SocketAddr,
    pub pkt: Result<Packet, rmp_serde::decode::Error>
}
#[derive(Debug)]
pub struct SendMessage {
    pub addr: SocketAddr,
    pub pkt: Packet
}
pub trait SendMessageExt {
    fn pkt_to(&self, m: Packet) -> SendMessage;
    fn cmd_to(&self, c: Command) -> SendMessage;
}
impl SendMessageExt for SocketAddr {
    fn pkt_to(&self, m: Packet) -> SendMessage {
        SendMessage {
            addr: self.clone(),
            pkt: m
        }
    }
    fn cmd_to(&self, c: Command) -> SendMessage {
        SendMessage {
            addr: self.clone(),
            pkt: Packet {
                id: 0,
                reply_to: 0,
                cmd: c
            }
        }
    }
}

pub struct SqaWireCodec;
impl UdpCodec for SqaWireCodec {
    type In = RecvMessage;
    type Out = SendMessage;
    fn decode(&mut self, src: &SocketAddr, buf: &[u8]) -> ::std::io::Result<Self::In> {
        let mut decoder = Deserializer::new(buf);
        Ok(RecvMessage {
            addr: src.clone(),
            pkt: Deserialize::deserialize(&mut decoder)
        })
    }
    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> SocketAddr {
        let _ = msg.pkt.serialize(&mut Serializer::new(buf));
        msg.addr
    }
}
