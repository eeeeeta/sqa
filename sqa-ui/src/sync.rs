use futures::sync::mpsc;
use std::sync::mpsc as smpsc;
use connection::{self, ConnectionState, ConnectionMessage};
use util::ThreadNotifier;
use tokio_core::reactor::{Handle, Remote};
use futures::{Poll, Async, Future, Stream};
use errors;

pub enum UIMessage {
    ConnState(ConnectionState)
}
pub enum BackendMessage {
    Connection(ConnectionMessage)
}
pub struct BackendContext {
    pub conn: connection::Context,
    pub tn: ThreadNotifier,
    pub rx: mpsc::UnboundedReceiver<BackendMessage>,
    pub tx: smpsc::Sender<UIMessage>,
    pub hdl: Handle
}
pub struct UIContext {
    pub rx: smpsc::Receiver<UIMessage>,
    pub tx: mpsc::UnboundedSender<BackendMessage>,
    pub conn: connection::ConnectionController,
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
            println!("FIXME: insert proper error handling here!\n{:?}", e);
        }
        Ok(Async::NotReady)
    }
}
impl UIContext {
    pub fn bind_all(&mut self) {
        self.conn.bind(&self.tx);
    }
    pub fn on_event(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            use self::UIMessage::*;
            match msg {
                ConnState(msg) => self.conn.on_msg(msg)
            }
        }
    }
}
