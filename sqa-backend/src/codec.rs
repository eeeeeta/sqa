use std::net::SocketAddr;
use tokio_core::net::{TcpStream, UdpCodec};
use rosc::{decoder, encoder, OscMessage, OscPacket, OscType};
use errors::*;
use mixer::MixerConf;
use errors::BackendErrorKind::*;
use undo::UndoState;
use actions::{ActionParameters, ActionMetadata, OpaqueAction};
use waveform::{WaveformRequest, WaveformReply};
use std::collections::HashMap;
use tokio_io::codec::length_delimited::Framed;
use futures::{Stream, Sink, Async, AsyncSink};
use uuid::Uuid;
use std::marker::PhantomData;
use std::convert::TryFrom;

type OscResult<T> = BackendResult<T>;
type OscError = BackendError;

#[derive(OscSerde, Serialize, Deserialize, Debug, Clone)]
pub enum Command {
    #[oscpath = "/ping"]
    Ping,
    #[oscpath = "/version"]
    Version,
    #[oscpath = "/subscribe"]
    Subscribe,
    #[oscpath = "/subscribe/associate"]
    SubscribeAndAssociate { #[ser] addr: SocketAddr },
    #[oscpath = "/actions/{typ}/new"]
    CreateAction { #[subst] typ: String },
    #[oscpath = "/actionlist"]
    ActionList,
    #[oscpath = "/action/{uuid}"]
    ActionInfo { #[subst] uuid: Uuid },
    #[oscpath = "/action/{uuid}/update"]
    UpdateActionParams { #[subst] uuid: Uuid, #[ser] params: ActionParameters, #[ser] desc: Option<String> },
    #[oscpath = "/action/{uuid}/updatemeta"]
    UpdateActionMetadata { #[subst] uuid: Uuid, #[ser] meta: ActionMetadata },
    #[oscpath = "/action/{uuid}/revive/{typ}"]
    ReviveAction { #[subst] uuid: Uuid, #[subst] typ: String, #[ser] meta: ActionMetadata, #[ser] params: ActionParameters },
    #[oscpath = "/action/{uuid}/create/{typ}"]
    CreateActionWithUuid { #[subst] typ: String, #[subst] uuid: Uuid },
    #[oscpath = "/action/{uuid}/create_extra/{typ}"]
    CreateActionWithExtras { #[subst] typ: String, #[subst] uuid: Uuid, #[ser] params: ActionParameters },
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
    #[oscpath = "/action/{uuid}/reset"]
    ResetAction { #[subst] uuid: Uuid },
    #[oscpath = "/action/{uuid}/pause"]
    PauseAction { #[subst] uuid: Uuid },
    #[oscpath = "/action/{uuid}/reorder"]
    ReorderAction { #[subst] uuid: Uuid, #[ser] new_pos: usize },
    #[oscpath = "/mixer/config"]
    GetMixerConf,
    #[oscpath = "/mixer/config/set"]
    SetMixerConf { #[ser] conf: MixerConf },
    #[oscpath = "/system/save"]
    MakeSavefile { #[verbatim = "string"] save_to: String },
    #[oscpath = "/system/load"]
    LoadSavefile { #[verbatim = "string"] load_from: String, #[verbatim = "bool"] force: bool },
    #[oscpath = "/system/undo"]
    Undo,
    #[oscpath = "/system/redo"]
    Redo,
    #[oscpath = "/system/undostate"]
    GetUndoState,
    #[oscpath = "/waveform/{uuid}/generate"]
    GenerateWaveform { #[subst] uuid: Uuid, #[ser] req: WaveformRequest }
}
#[derive(OscSerde, Serialize, Deserialize, Debug, Clone)]
pub enum Reply {
    #[oscpath = "/pong"]
    Pong,
    #[oscpath = "/reply/version"]
    ServerVersion { #[verbatim = "string"] ver: String },
    #[oscpath = "/reply/subscribe"]
    Subscribed,
    #[oscpath = "/reply/subscribe/associate"]
    Associated { #[ser] res: Result<(), String> },
    #[oscpath = "/error/deserfail"]
    DeserFailed { #[verbatim = "string"] err: String },

    #[oscpath = "/reply/actions/create"]
    ActionCreated { #[ser] res: Result<Uuid, String> },
    #[oscpath = "/reply/action/{uuid}"]
    ActionInfoRetrieved { #[subst] uuid: Uuid, #[ser] res: Result<OpaqueAction, String> },
    #[oscpath = "/reply/action/{uuid}/update"]
    ActionParamsUpdated { #[subst] uuid: Uuid, #[ser] res: Result<(), String> },
    #[oscpath = "/reply/action/{uuid}/updatemeta"]
    ActionMetadataUpdated { #[subst] uuid: Uuid, #[ser] res: Result<(), String> },
    #[oscpath = "/reply/action/{uuid}/delete"]
    ActionDeleted { #[subst] uuid: Uuid, #[ser] deleted: bool },
    #[oscpath = "/reply/action/{uuid}/load"]
    ActionLoaded { #[subst] uuid: Uuid, #[ser] res: Result<(), String> },
    #[oscpath = "/reply/action/{uuid}/execute"]
    ActionExecuted { #[subst] uuid: Uuid, #[ser] res: Result<(), String> },
    #[oscpath = "/reply/action/{uuid}/pause"]
    ActionMaybePaused { #[subst] uuid: Uuid, #[ser] res: Result<(), String> },
    #[oscpath = "/reply/action/{uuid}/reset"]
    ActionReset { #[subst] uuid: Uuid, #[ser] res: Result<(), String> },
    #[oscpath = "/reply/action/{uuid}/reorder"]
    ActionReordered { #[subst] uuid: Uuid, #[ser] res: Result<(), String> },
    #[oscpath = "/reply/mixer/config"]
    MixerConfSet { #[ser] res: Result<(), String> },
    #[oscpath = "/reply/actionlist"]
    ReplyActionList { #[ser] list: HashMap<Uuid, OpaqueAction>, #[ser] order: Vec<Uuid> },
    #[oscpath = "/update/order"]
    UpdateOrder { #[ser] order: Vec<Uuid> },
    #[oscpath = "/update/action/{uuid}"]
    UpdateActionInfo { #[subst] uuid: Uuid, #[ser] data: OpaqueAction },
    #[oscpath = "/update/action/{uuid}/delete"]
    UpdateActionDeleted { #[subst] uuid: Uuid },
    #[oscpath = "/update/mixer/config"]
    UpdateMixerConf { #[ser] conf: MixerConf },
    #[oscpath = "/reply/system/save"]
    SavefileMade { #[ser] res: Result<(), String> },
    #[oscpath = "/reply/system/load"]
    SavefileLoaded { #[ser] res: Result<(), String> },
    #[oscpath = "/reply/system/undostate"]
    ReplyUndoState { #[ser] ctx: UndoState },
    #[oscpath = "/reply/waveform/{uuid}/generated"]
    WaveformGenerated { #[subst] uuid: Uuid, #[ser] res: Result<WaveformReply, String> },
    #[oscpath = "/error/oversized"]
    OversizedReply
}
/// A decoded `Command` (which may or may not have succeeded), and where it came
/// from.
#[derive(Debug)]
pub struct RecvMessage {
    /// Where the message came from.
    pub addr: SocketAddr,
    /// The result of message decoding: the string denotes the OSC path invoked.
    pub pkt: BackendResult<(String, Command)>
}
/// An OSC message, with an address it should be sent to.
#[derive(Debug, Clone)]
pub struct SendMessage {
    pub addr: SocketAddr,
    pub pkt: OscMessage
}
/// Some bytes, with an address they should be sent to.
pub struct SendBytes {
    pub addr: SocketAddr,
    pub pkt: Vec<u8>
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
    type Out = Vec<u8>;
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
        ::std::mem::replace(buf, msg);
        self.addr
    }
}
pub struct SqaWireCodec;
impl UdpCodec for SqaWireCodec {
    type In = RecvMessage;
    type Out = SendBytes;
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
        let SendBytes { pkt, addr } = msg;
        ::std::mem::replace(buf, pkt);
        addr
    }
}
pub struct SqaTcpStream<O> {
    pub inner: Framed<TcpStream, Vec<u8>>,
    _p1: PhantomData<O>
}
impl<O> SqaTcpStream<O>
    where O: TryFrom<OscMessage, Error=BackendError> {
    pub fn new(st: TcpStream) -> Self {
        Self {
            inner: Framed::new(st),
            _p1: PhantomData
        }
    }
}
impl<O> Stream for SqaTcpStream<O>
    where O: TryFrom<OscMessage, Error=BackendError> {
    type Item = BackendResult<O>;
    type Error = BackendError;

    fn poll(&mut self) -> BackendResult<Async<Option<Self::Item>>> {
        match self.inner.poll()? {
            Async::Ready(buf) => {
                match buf {
                    Some(buf) => {
                        let ret = match decoder::decode(&buf) {
                            Ok(pkt) => {
                                match pkt {
                                    OscPacket::Message(m) => {
                                        match O::try_from(m) {
                                            Ok(o) => Ok(o),
                                            Err(e) => Err(e.into())
                                        }
                                    },
                                    _ => Err(BackendErrorKind::UnsupportedOSCBundle.into())
                                }
                            },
                            Err(e) => Err(e.into())
                        };
                        Ok(Async::Ready(Some(ret)))
                    },
                    None => Ok(Async::Ready(None))
                }
            },
            Async::NotReady => Ok(Async::NotReady)
        }
    }
}
impl<O> Sink for SqaTcpStream<O>
    where O: TryFrom<OscMessage, Error=BackendError> {
    type SinkItem = OscMessage;
    type SinkError = BackendError;

    fn start_send(&mut self, rpl: OscMessage) -> BackendResult<AsyncSink<OscMessage>> {
        let pkt = encoder::encode(&OscPacket::Message(rpl.clone()))?;
        Ok(match self.inner.start_send(pkt)? {
            AsyncSink::NotReady(_) => AsyncSink::NotReady(rpl),
            AsyncSink::Ready => AsyncSink::Ready
        })
    }
    fn poll_complete(&mut self) -> BackendResult<Async<()>> {
        Ok(self.inner.poll_complete()?)
    }
}
