use futures::sync::mpsc;
use tokio_core::net::{UdpFramed, UdpSocket};
use tokio_core::reactor::{Timeout, Remote};
use sqa_backend::VERSION;
use sqa_backend::codec::{SqaClientCodec, Reply, Command};
use gtk::prelude::*;
use gtk::{Label, Button, Popover, Entry, Builder};
use std::net::SocketAddr;
use futures::{Sink, Stream, Poll, Async, Future};
use sync::{BackendContextArgs, BackendMessage, UIMessage};
use errors;
use time;
use std::mem;
use std::time::Duration;
use std::default::Default;

#[derive(Clone, Debug)]
pub enum ConnectionState {
    Disconnected,
    VersionQuerySent { addr: SocketAddr },
    SubscriptionQuerySent { addr: SocketAddr, ver: String },
    Connected { addr: SocketAddr, ver: String, last_ping: u64, last_err: Option<String> },
    RecvFailed(String),
    RecvThreadFailed
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
        self.send(Command::Ping)?;
        self.timeout = Some(Timeout::new(Duration::new(10, 0), &args.hdl)?);
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
    header_lbl: Label,
    status_lbl: Label,
    popover_btn: Button,
    popover: Popover,
    connect_btn: Button,
    disconnect_btn: Button,
    ip_entry: Entry,
    version_lbl: Label,
}

impl ConnectionController {
    pub fn new(b: &Builder) -> Self {
        build!(ConnectionController
               using b
               get header_lbl, status_lbl, popover_btn, popover, connect_btn,
               disconnect_btn, ip_entry, version_lbl)
    }
    pub fn bind(&mut self, tx: &mpsc::UnboundedSender<BackendMessage>) {
        let pop = self.popover.clone();
        self.popover_btn.connect_clicked(move |_| {
            pop.show_all();
        });
        self.disconnect_btn.connect_clicked(clone!(tx; |_a| {
            mpsc::UnboundedSender::send(&tx, BackendMessage::Connection(ConnectionMessage::Disconnect));
        }));
        let ipe = self.ip_entry.clone();
        self.connect_btn.connect_clicked(clone!(tx; |_a| {
            if let Ok(addr) = ipe.get_text().unwrap_or("".into()).parse() {
                mpsc::UnboundedSender::send(&tx,
                                            BackendMessage::Connection(ConnectionMessage::Connect(addr)));
            }
        }));
    }
    pub fn on_msg(&mut self, msg: ConnectionState) {
        use self::ConnectionState::*;
        match msg {
            Disconnected => {
                self.status_lbl.set_text("Disconnected. Enter a server IP to connect.");
                self.version_lbl.set_text("Enter an IP below!");
            },
            VersionQuerySent { addr } => {
                self.status_lbl.set_text(&format!("Connecting to {} (sent version query)...", addr));
            },
            SubscriptionQuerySent { addr, ver } => {
                self.status_lbl.set_text(&format!("Version of {} is {}. Connecting...", addr, ver));
            },
            Connected { addr, ver, .. } => {
                self.status_lbl.set_text(&format!("Connected to {}.", addr));
                self.version_lbl.set_text(&format!("Server version: {}", ver));
            },
            _ => {}
        }
    }
}
