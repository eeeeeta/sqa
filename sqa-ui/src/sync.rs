use futures::sync::mpsc;
use std::sync::mpsc as smpsc;
use connection::{self, ConnectionState, ConnectionMessage, ConnectionUIMessage};
use sqa_backend::mixer::MixerConf;
use sqa_backend::codec::{Reply, Command};
use util::ThreadNotifier;
use tokio_core::reactor::Handle;
use futures::{Poll, Async, Future, Stream};
use messages;
use errors;
use actions;
use save;
use copy;
use config;

pub enum UIMessage {
    ConnState(ConnectionState),
    ConnMessage(ConnectionUIMessage),
    ActionReply(Reply),
    ActionMessage(actions::ActionMessage),
    ActionInternal(actions::ActionInternalMessage),
    UpdatedMixerConf(MixerConf),
    Message(messages::Message),
    Save(save::SaveMessage),
    Copy(copy::CopyPasteMessage),
    Config(config::ConfigMessage)
}
pub enum BackendMessage {
    Connection(ConnectionMessage)
}
impl From<Command> for BackendMessage {
    fn from(c: Command) -> BackendMessage {
        BackendMessage::Connection(ConnectionMessage::Send(c))
    }
}
message_impls!(
    UIMessage,
    ConnState, ConnectionState,
    ConnMessage, ConnectionUIMessage,
    ActionReply, Reply,
    ActionMessage, actions::ActionMessage,
    ActionInternal, actions::ActionInternalMessage,
    UpdatedMixerConf, MixerConf,
    Message, messages::Message,
    Save, save::SaveMessage,
    Copy, copy::CopyPasteMessage,
    Config, config::ConfigMessage
);
message_impls!(
    BackendMessage,
    Connection, ConnectionMessage
);

pub struct BackendContext {
    pub conn: connection::Context,
    pub tn: ThreadNotifier,
    pub rx: mpsc::UnboundedReceiver<BackendMessage>,
    pub tx: smpsc::Sender<UIMessage>,
    pub hdl: Handle
}
pub struct UIContext {
    pub rx: smpsc::Receiver<UIMessage>,
    pub stx: smpsc::Sender<UIMessage>,
    pub stn: ThreadNotifier,
    pub tx: mpsc::UnboundedSender<BackendMessage>,
    pub conn: connection::ConnectionController,
    pub act: actions::ActionController,
    pub msg: messages::MessageController,
    pub save: save::SaveController,
    pub copy: copy::CopyPasteController,
    pub config: config::ConfigController
}
#[derive(Clone)]
pub struct UISender {
    backend: mpsc::UnboundedSender<BackendMessage>,
    stx: smpsc::Sender<UIMessage>,
    stn: ThreadNotifier
}
impl UISender {
    pub fn send<T: Into<BackendMessage>>(&self, obj: T) {
        self.backend.send(obj.into())
            .expect("RIP in pepperoni, backend");
    }
    pub fn send_internal<T: Into<UIMessage>>(&self, obj: T) {
        self.stx.send(obj.into())
            .expect("RIP in pepperoni....myself?!");
        self.stn.notify();
    }
}
pub struct BackendContextArgs<'a> {
    pub tn: &'a mut ThreadNotifier,
    pub tx: &'a mut smpsc::Sender<UIMessage>,
    pub hdl: Handle
}
impl<'a> BackendContextArgs<'a> {
    pub fn send(&mut self, msg: UIMessage) {
        self.tx.send(msg)
            .expect("RIP in pepperoni, frontend");
        self.tn.notify();
    }
}
impl Future for BackendContext {
    type Item = ();
    type Error = errors::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.rx.poll() {
                Ok(Async::Ready(Some(msg))) => {
                    use self::BackendMessage::*;
                    match msg {
                        Connection(msg) => self.conn.add_msg(msg)
                    }
                },
                _ => break
            }
        }
        let args = BackendContextArgs {
            tn: &mut self.tn,
            tx: &mut self.tx,
            hdl: self.hdl.clone()
        };
        if let Err(e) = self.conn.poll(args) {
            error!("FIXME: insert proper error handling here!\n{:?}", e);
        }
        Ok(Async::NotReady)
    }
}
impl UIContext {
    pub fn bind_all(&mut self) {
        let uis = UISender {
            stx: self.stx.clone(),
            stn: self.stn.clone(),
            backend: self.tx.clone()
        };
        self.conn.bind(&uis);
        self.act.bind(&uis);
        self.msg.bind(&uis);
        self.save.bind(&uis);
        self.copy.bind(&uis);
        self.config.bind(&uis);
    }
    pub fn on_event(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            use self::UIMessage::*;
            match msg {
                ConnState(msg) => self.conn.on_state_change(msg),
                ConnMessage(msg) => self.conn.on_msg(msg),
                ActionReply(rpl) => self.act.on_action_reply(rpl),
                ActionInternal(msg) => self.act.on_internal(msg),
                ActionMessage(msg) => self.act.on_action_msg(msg),
                UpdatedMixerConf(cnf) => self.act.on_mixer(cnf),
                Message(msg) => self.msg.on_message(msg),
                Save(msg) => self.save.on_message(msg),
                Copy(msg) => self.copy.on_message(msg),
                Config(msg) => self.config.on_message(msg)
            }
        }
    }
}
