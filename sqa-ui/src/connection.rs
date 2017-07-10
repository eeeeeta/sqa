use tokio_core::net::{UdpFramed, UdpSocket};
use tokio_core::reactor::{Timeout};
use sqa_backend::codec::{SqaClientCodec, Reply, Command};
use gtk::prelude::*;
use gtk::{Button, Builder, MenuItem, IconSize, Image};
use std::net::SocketAddr;
use futures::{Sink, Stream, Async, Future};
use sync::{BackendContextArgs, UIMessage, UISender};
use widgets::{PropertyWindow, FallibleEntry};
use errors;
use messages::Message;
use time;
use save::SaveMessage;
use std::mem;
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum ConnectionState {
    Disconnected,
    VersionQuerySent { addr: SocketAddr },
    SubscriptionQuerySent { addr: SocketAddr, ver: String },
    Connected { addr: SocketAddr, ver: String, last_ping: u64, last_pong: u64, last_err: Option<String> }
}
pub enum ConnectionUIMessage {
    ConnectClicked,
    Show,
    NewlyConnected,
    NewlyDisconnected,
    UndoState(UndoState)
}
pub enum ConnectionMessage {
    Disconnect,
    Connect(SocketAddr),
    Send(Command),
    Perform(UndoableChange),
    Undo,
    Redo
}
#[derive(Debug)]
pub struct UndoableChange {
    pub undo: Command,
    pub redo: Command,
    pub desc: String
}
#[derive(Clone, Debug)]
pub struct UndoState {
    pub undo: Option<String>,
    pub redo: Option<String>
}
pub struct UndoContext {
    changes: Vec<UndoableChange>,
    idx: Option<usize>
}
impl UndoContext {
    pub fn new() -> Self {
        let gutter = UndoableChange {
            undo: Command::Ping,
            redo: Command::Ping,
            desc: "nothing".into()
        };
        UndoContext {
            changes: vec![gutter],
            idx: None
        }
    }
    pub fn register_change(&mut self, ch: UndoableChange) {
        trace!("registering undoable change {:?}", ch);
        if let Some(idx) = self.idx {
            trace!("obliterating redoability");
            self.changes.drain((idx+1)..);
            self.idx = None;
        }
        self.changes.push(ch);
    }
    fn indexes(&self) -> (Option<usize>, Option<usize>) {
        let (mut undo, mut redo) = (None, None);
        let idx = self.idx.unwrap_or(self.changes.len()-1);
        if self.changes.get(idx+1).is_some() {
            redo = Some(idx+1);
        }
        if self.changes[idx].desc != "nothing" {
            undo = Some(idx);
        }
        (undo, redo)
    }
    pub fn undo(&mut self) -> Option<Command> {
        let (undo, _) = self.indexes();
        trace!("attempting to undo, idx {:?}", self.idx);
        if let Some(idx) = undo {
            self.idx = Some(idx-1);
            Some(self.changes[idx].undo.clone())
        }
        else {
            None
        }
    }
    pub fn redo(&mut self) -> Option<Command> {
        let (_, redo) = self.indexes();
        trace!("attempting to redo, idx {:?}", self.idx);
        if let Some(idx) = redo {
            if idx == self.changes.len()-1 {
                self.idx = None;
            }
            else {
                self.idx = Some(idx);
            }
            Some(self.changes[idx].redo.clone())
        }
        else {
            None
        }
    }
    pub fn state(&self) -> UndoState {
        let (undo, redo) = self.indexes();
        UndoState {
            undo: undo.and_then(|idx| self.changes.get(idx)).map(|x| x.desc.clone()),
            redo: redo.and_then(|idx| self.changes.get(idx)).map(|x| x.desc.clone())
        }
    }
}
pub struct Context {
    state: ConnectionState,
    messages: Vec<ConnectionMessage>,
    sock: Option<UdpFramed<SqaClientCodec>>,
    timeout: Option<Timeout>,
    uc: UndoContext
}
impl Context {
    pub fn new() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            messages: vec![],
            sock: None,
            timeout: None,
            uc: UndoContext::new()
        }
    }
    fn notify_state_change(&mut self, args: &mut BackendContextArgs) {
        debug!("State change: {:?}", self.state);
        args.send(UIMessage::ConnState(self.state.clone()));
    }
    fn send(&mut self, cmd: Command) -> errors::Result<()> {
        self.sock.as_mut()
            .expect("Connected with no socket")
            .start_send(cmd)?;
        self.sock.as_mut()
            .expect("Connected with no socket")
            .poll_complete()?;
        Ok(())
    }
    fn update_last_ping(&mut self) {
        if let ConnectionState::Connected { ref mut last_ping, .. } = self.state {
            *last_ping = time::precise_time_ns();
        }
    }
    fn ping_timeout(&mut self, args: &mut BackendContextArgs) -> errors::Result<()> {
        if let ConnectionState::Connected { last_ping, last_pong, .. } = self.state {
            if last_pong < last_ping {
                info!("Disconnected from server due to ping timeout.");
                self.sock.take();
                self.state = ConnectionState::Disconnected;
                args.send(ConnectionUIMessage::NewlyDisconnected.into());
                self.notify_state_change(args);
            }
            else {
                self.send(Command::Ping)?;
                trace!("sending ping");
                self.update_last_ping();
                self.notify_state_change(args);
            }
            let mut tm = Timeout::new(Duration::new(10, 0), &args.hdl)?;
            tm.poll()?;
            self.timeout = Some(tm);
        }
        Ok(())
    }
    fn handle_external_nonstateful(&mut self, msg: Reply, args: &mut BackendContextArgs) -> errors::Result<()> {
        use self::Reply::*;
        match msg {
            x @ ActionCreated {..} |
            x @ ActionInfoRetrieved {..} |
            x @ ActionParamsUpdated {..} |
            x @ ActionDeleted {..} |
            x @ ActionLoaded {..} |
            x @ ActionExecuted {..} |
            x @ UpdateActionInfo {..} |
            x @ UpdateActionDeleted {..} |
            x @ ReplyActionList {..} => {
                args.send(UIMessage::ActionReply(x));
            },
            UpdateMixerConf { conf } => {
                args.send(UIMessage::UpdatedMixerConf(conf));
            },
            x @ SavefileMade {..} |
            x @ SavefileLoaded {..} => {
                args.send(UIMessage::Save(SaveMessage::External(x)));
            },
            _ => {}
        }
        Ok(())
    }
    fn handle_external(&mut self, msg: Reply, args: &mut BackendContextArgs) -> errors::Result<bool> {
        use self::ConnectionState::*;
        match mem::replace(&mut self.state, Disconnected) {
            VersionQuerySent { addr } => {
                if let Reply::ServerVersion { ver } = msg {
                    self.send(Command::Subscribe)?;
                    self.state = SubscriptionQuerySent { addr, ver };
                    Ok(true)
                }
                else {
                    self.state = VersionQuerySent { addr };
                    Ok(false)
                }
            },
            SubscriptionQuerySent { addr, ver } => {
                if let Reply::Subscribed = msg {
                    let last_ping = time::precise_time_ns();
                    let last_pong = time::precise_time_ns();
                    let last_err = None;
                    let ver = ver.clone(); // FIXME: not ideal :p
                    self.state = Connected { addr, ver, last_ping, last_err, last_pong };
                    self.ping_timeout(args)?;
                    self.send(Command::GetMixerConf)?;
                    self.send(Command::ActionList)?;
                    args.send(ConnectionUIMessage::NewlyConnected.into());
                    Ok(true)
                }
                else {
                    self.state = SubscriptionQuerySent { addr, ver };
                    Ok(false)
                }
            },
            Connected { addr, ver, last_ping, mut last_pong, last_err } => {
                if let Reply::Pong = msg {
                    last_pong = time::precise_time_ns();
                    trace!("got pong");
                    self.state = Connected { addr, ver, last_ping, last_pong, last_err };
                    Ok(true)
                }
                else {
                    self.state = Connected { addr, ver, last_ping, last_pong, last_err };
                    self.handle_external_nonstateful(msg, args)?;
                    Ok(false)
                }
            },
            _ => {
                Ok(false)
            }
        }
    }
    fn handle_internal(&mut self, msg: ConnectionMessage, args: &mut BackendContextArgs) -> errors::Result<bool> {
        use self::ConnectionMessage::*;
        match msg {
            Disconnect => {
                self.sock.take();
                self.state = ConnectionState::Disconnected;
                Ok(true)
            },
            Connect(addr) => {
                self.sock.take();
                let recv_addr = "127.0.0.1:53001".parse().unwrap();
                let codec = SqaClientCodec::new(addr);
                let sock = UdpSocket::bind(&recv_addr, &args.hdl)?;
                let mut sock = sock.framed(codec);
                sock.start_send(Command::Version)?;
                self.sock = Some(sock);
                self.state = ConnectionState::VersionQuerySent { addr: addr };
                Ok(true)
            },
            Send(cmd) => {
                if let ConnectionState::Connected { .. } = self.state {
                    self.send(cmd)?;
                }
                else {
                    args.send(Message::Error("Not connected, but tried to send messages.".into()).into());
                }
                Ok(false)
            },
            Perform(ua) => {
                if let ConnectionState::Connected { .. } = self.state {
                    self.send(ua.redo.clone())?;
                    self.uc.register_change(ua);
                    args.send(ConnectionUIMessage::UndoState(self.uc.state()).into());
                }
                else {
                    args.send(Message::Error("Not connected, but tried to send messages.".into()).into());
                }
                Ok(false)
            },
            x @ Undo | x @ Redo => {
                if let ConnectionState::Connected { .. } = self.state {
                    let (act, msg) = match x {
                        Undo => (self.uc.undo(), "undo"),
                        Redo => (self.uc.redo(), "redo"),
                        _ => unreachable!()
                    };
                    if let Some(act) = act {
                        self.send(act)?;
                        args.send(Message::Statusbar(format!("Action {}ne.", msg)).into());
                        args.send(ConnectionUIMessage::UndoState(self.uc.state()).into());
                    }
                    else {
                        args.send(Message::Error(format!("Nothing to {}.", msg)).into());
                    }
                }
                else {
                    args.send(Message::Error("Not connected, but tried to undo/redo.".into()).into());
                }
                Ok(false)
            },
        }
    }
    pub fn add_msg(&mut self, msg: ConnectionMessage) {
        self.messages.push(msg);
    }
    pub fn poll(&mut self, mut args: BackendContextArgs) -> errors::Result<()> {
        let msgs = self.messages.drain(..).collect::<Vec<_>>();
        for message in msgs {
            if self.handle_internal(message, &mut args)? {
                self.notify_state_change(&mut args);
            }
        }
        loop {
            if let Some(data) = self.sock.as_mut().map(|s| s.poll()) {
                match data {
                    Ok(Async::Ready(Some(res))) => {
                        let res = match res {
                            Ok(r) => r,
                            Err(e) => {
                                args.send(Message::Error(format!("Error deserialising reply from server: {}", e)).into());
                                continue
                            }
                        };
                        if self.handle_external(res, &mut args)? {
                            self.notify_state_change(&mut args);
                        }
                    },
                    Ok(Async::Ready(None)) => unreachable!(),
                    Ok(Async::NotReady) => break,
                    Err(e) => return Err(e.into())
                }
            }
            else {
                break
            }
        }
        if let Some(ref mut sock) = self.sock {
            sock.poll_complete()?;
        }
        if let Some(data) = self.timeout.as_mut().map(|s| s.poll()) {
            match data {
                Ok(Async::Ready(_)) => {
                    self.ping_timeout(&mut args)?;
                },
                Err(e) => return Err(e.into()),
                _ => {}
            }
        }
        Ok(())
    }
}
pub struct ConnectionController {
    pub pwin: PropertyWindow,
    ipe: FallibleEntry,
    connect_btn: Button,
    disconnect_btn: Button,
    status_btn: Button,
    status_img: Image,
    mundo: MenuItem,
    mredo: MenuItem,
    tx: Option<UISender>,
    state: ConnectionState,
    menuitem: MenuItem
}

impl ConnectionController {
    pub fn new(b: &Builder) -> Self {
        let mut pwin = PropertyWindow::new();
        let ipe = FallibleEntry::new();
        let connect_btn = Button::new_with_mnemonic("_Connect");
        let disconnect_btn = Button::new_with_mnemonic("_Disconnect");
        pwin.append_property("IP address and port", &*ipe);
        pwin.append_button(&connect_btn);
        pwin.append_button(&disconnect_btn);
        let tx = None;
        let state = ConnectionState::Disconnected;
        let mut ret = build!(ConnectionController using b
                             with pwin, ipe, connect_btn, disconnect_btn, tx, state
                             get menuitem, status_btn, status_img, mundo, mredo);
        ret.on_state_change(ConnectionState::Disconnected);
        ret
    }
    pub fn bind(&mut self, tx: &UISender) {
        self.disconnect_btn.connect_clicked(clone!(tx; |_| {
            tx.send(ConnectionMessage::Disconnect);
        }));
        self.connect_btn.connect_clicked(clone!(tx; |_| {
            tx.send_internal(ConnectionUIMessage::ConnectClicked);
        }));
        self.ipe.on_enter(clone!(tx; |_| {
            tx.send_internal(ConnectionUIMessage::ConnectClicked);
        }));
        self.menuitem.connect_activate(clone!(tx; |_| {
            tx.send_internal(ConnectionUIMessage::Show);
        }));
        self.status_btn.connect_clicked(clone!(tx; |_| {
            tx.send_internal(ConnectionUIMessage::Show);
        }));
        self.mundo.connect_activate(clone!(tx; |_| {
            tx.send(ConnectionMessage::Undo);
        }));
        self.mredo.connect_activate(clone!(tx; |_| {
            tx.send(ConnectionMessage::Redo);
        }));
        self.tx = Some(tx.clone());
    }
    pub fn on_msg(&mut self, msg: ConnectionUIMessage) {
        use self::ConnectionUIMessage::*;
        match msg {
            ConnectClicked => {
                match self.ipe.get_text().parse() {
                    Ok(addr2) => {
                        if let ConnectionState::Connected { addr, .. } = self.state {
                            if addr2 == addr {
                                return;
                            }
                        }
                        self.tx.as_mut().unwrap()
                            .send(ConnectionMessage::Connect(addr2));
                    },
                    Err(e) => {
                        self.ipe.throw_error(&e.to_string());
                    },
                }
            },
            Show => self.pwin.window.show_all(),
            NewlyConnected => {
                self.pwin.window.hide();
                self.tx.as_mut().unwrap()
                    .send_internal(Message::Statusbar("Connected to server.".into()));
            },
            NewlyDisconnected => {
                self.tx.as_mut().unwrap().
                    send_internal(Message::Statusbar("Disconnected from server.".into()));
                self.pwin.window.show_all();
            },
            UndoState(st) => {
                if let Some(dsc) = st.undo {
                    self.mundo.set_sensitive(true);
                    self.mundo.set_label(&format!("Undo {}", dsc));
                }
                else {
                    self.mundo.set_sensitive(false);
                    self.mundo.set_label("Undo");
                }
                if let Some(dsc) = st.redo {
                    self.mredo.set_sensitive(true);
                    self.mredo.set_label(&format!("Redo {}", dsc));
                }
                else {
                    self.mredo.set_sensitive(false);
                    self.mredo.set_label("Redo");
                }
            }
        }
    }
    pub fn on_state_change(&mut self, msg: ConnectionState) {
        use self::ConnectionState::*;
        self.state = msg.clone();
        match msg {
            Disconnected => {
                self.pwin.update_header(
                    "gtk-disconnect",
                    "Disconnected",
                    "Enter an IP address to connect."
                );
                self.status_img.set_from_stock("gtk-disconnect", IconSize::Button.into());
                self.status_btn.set_label("disconnected");
            },
            VersionQuerySent { addr } => {
                self.pwin.update_header(
                    "gtk-refresh",
                    "Connecting (0%)...",
                    format!("Connecting to {} (sent version query)...", addr)
                );
                self.status_img.set_from_stock("gtk-refresh", IconSize::Button.into());
                self.status_btn.set_label("connecting");
            },
            SubscriptionQuerySent { addr, ver } => {
                self.pwin.update_header(
                    "gtk-refresh",
                    "Connecting (50%)...",
                    format!("Version of {} is {}. Connecting...", addr, ver)
                );
                self.status_img.set_from_stock("gtk-refresh", IconSize::Button.into());
                self.status_btn.set_label("connecting");
            },
            Connected { addr, ver, last_ping, last_pong, .. } => {
                let ping = if last_ping > last_pong {
                    format!("...")
                } else {
                    format!("{:.2}ms", (((last_pong - last_ping) / 1000) as f64) / 1000.0)
                };
                self.pwin.update_header(
                    "gtk-connect",
                    format!("Connected (ping: {})", ping),
                    format!("Connected to {}, version: {}", addr, ver)
                );
                self.status_img.set_from_stock("gtk-connect", IconSize::Button.into());
                self.status_btn.set_label(&format!("ping: {}", ping));
            }
        }
    }
}
