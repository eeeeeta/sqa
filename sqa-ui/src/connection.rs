use tokio_core::net::{UdpFramed, UdpSocket};
use tokio_core::reactor::{Timeout};
use sqa_backend::codec::{SqaClientCodec, Reply, Command};
use gtk::prelude::*;
use gtk::{Button, Builder};
use std::net::SocketAddr;
use futures::{Sink, Stream, Async, Future};
use sync::{BackendContextArgs, UIMessage, UISender};
use widgets::{PropertyWindow, FallibleEntry};
use errors;
use time;
use std::mem;
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum ConnectionState {
    Disconnected,
    VersionQuerySent { addr: SocketAddr },
    SubscriptionQuerySent { addr: SocketAddr, ver: String },
    Connected { addr: SocketAddr, ver: String, last_ping: u64, last_err: Option<String> },
    RecvFailed(String),
    RecvThreadFailed
}
pub enum ConnectionUIMessage {
    ConnectClicked
}
pub enum ConnectionMessage {
    Disconnect,
    Connect(SocketAddr),
    Send(Command)
}
pub struct Context {
    state: ConnectionState,
    messages: Vec<ConnectionMessage>,
    sock: Option<UdpFramed<SqaClientCodec>>,
    timeout: Option<Timeout>
}
impl Context {
    pub fn new() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            messages: vec![],
            sock: None,
            timeout: None
        }
    }
    fn notify_state_change(&mut self, args: &mut BackendContextArgs) {
        println!("State change: {:?}", self.state);
        args.send(UIMessage::ConnState(self.state.clone()));
    }
    fn send(&mut self, cmd: Command) -> errors::Result<()> {
        self.sock.as_mut()
            .expect("Connected with no socket")
            .start_send(cmd)?;
        Ok(())
    }
    fn ping_timeout(&mut self, args: &mut BackendContextArgs) -> errors::Result<()> {
        if let ConnectionState::Connected {..} = self.state {
            self.send(Command::Ping)?;
            self.timeout = Some(Timeout::new(Duration::new(10, 0), &args.hdl)?);
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
            x @ UpdateActionDeleted {..} => {
                args.send(UIMessage::ActionReply(x));
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
                    let last_err = None;
                    let ver = ver.clone(); // FIXME: not ideal :p
                    self.ping_timeout(args)?;
                    self.state = Connected { addr, ver, last_ping, last_err };
                    Ok(true)
                }
                else {
                    self.state = SubscriptionQuerySent { addr, ver };
                    Ok(false)
                }
            },
            x => {
                self.state = x;
                self.handle_external_nonstateful(msg, args)?;
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
                Ok(false)
            }
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
                        if self.handle_external(res?, &mut args)? {
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
    tx: Option<UISender>,
    state: ConnectionState
}

impl ConnectionController {
    pub fn new(b: &Builder) -> Self {
        let mut pwin = PropertyWindow::new(b);
        let ipe = FallibleEntry::new(b);
        let connect_btn = Button::new_with_mnemonic("_Connect");
        let disconnect_btn = Button::new_with_mnemonic("_Disconnect");
        pwin.append_property("IP address and port", &*ipe);
        pwin.append_button(&connect_btn);
        pwin.append_button(&disconnect_btn);
        let tx = None;
        let state = ConnectionState::Disconnected;
        let mut ret = ConnectionController { pwin, ipe, connect_btn, disconnect_btn, tx, state };
        ret.on_state_change(ConnectionState::Disconnected);
        ret
    }
    pub fn bind(&mut self, tx: &UISender) {
        self.disconnect_btn.connect_clicked(clone!(tx; |_a| {
            tx.send(ConnectionMessage::Disconnect);
        }));
        self.connect_btn.connect_clicked(clone!(tx; |_a| {
            tx.send_internal(ConnectionUIMessage::ConnectClicked);
        }));
        self.ipe.on_enter(clone!(tx; |_a| {
            tx.send_internal(ConnectionUIMessage::ConnectClicked);
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
                        self.ipe.throw_error(e.to_string());
                    },
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
            },
            VersionQuerySent { addr } => {
                self.pwin.update_header(
                    "gtk-refresh",
                    "Connecting (0%)...",
                    format!("Connecting to {} (sent version query)...", addr)
                );
            },
            SubscriptionQuerySent { addr, ver } => {
                self.pwin.update_header(
                    "gtk-refresh",
                    "Connecting (50%)...",
                    format!("Version of {} is {}. Connecting...", addr, ver)
                );
            },
            Connected { addr, ver, .. } => {
                self.pwin.update_header(
                    "gtk-connect",
                    "Connected",
                    format!("Connected to {}, version: {}", addr, ver)
                );
            },
            _ => {}
        }
    }
}
