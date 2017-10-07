use tokio_core::net::{UdpFramed, UdpSocket, TcpStream, TcpStreamNew};
use tokio_core::reactor::{Timeout};
use sqa_backend::codec::{SqaClientCodec, SqaTcpStream, Reply, Command};
use sqa_backend::undo::{UndoState};
use sqa_backend::handlers::UDP_MAX_PACKET_SIZE;
use gtk::prelude::*;
use gtk::{Button, Builder, MenuItem, CheckMenuItem, IconSize, Window, Image};
use std::net::SocketAddr;
use futures::{Sink, Stream, Async, Future};
use sync::{BackendContextArgs, UIMessage, UISender};
use widgets::{PropertyWindow, FallibleEntry};
use errors;
use messages::Message;
use time;
use save::SaveMessage;
use config::ConfigMessage;
use std::time::Duration;
use glib::signal;

/// A FSM representing the current connection state.
#[derive(Clone, Debug)]
pub enum ConnectionState {
    /// We're currently disconnected from the server.
    Disconnected,
    /// We're in the process of connecting to the server.
    Connecting {
        /// The server's UDP address.
        addr: SocketAddr,
        /// A string describing the current connection progress.
        status: &'static str
    },
    Connected {
        addr: SocketAddr,
        ver: String,
        /// A timestamp representing the last time a `Ping` was sent.
        last_ping: u64,
        /// A timestamp representing the last time a `Pong` was received.
        last_pong: u64
    }
}
/// Struct for 'an active UDP connection'.
struct UdpHandle {
    framed: UdpFramed<SqaClientCodec>,
    addr: SocketAddr
}
/// Private FSM that holds connection objects.
///
/// We go through this in order of variant declaration to connect to a server.
#[derive(is_enum_variant)]
enum PrivateConnectionState {
    /// No active connections whatsoever.
    Disconnected,
    /// Trying to establish a TCP connection.
    TcpConnecting { tcp: TcpStreamNew, addr: SocketAddr },
    /// TCP connection established - sending `Version` query to server over UDP.
    VersionQuerySent { udp: UdpHandle, tcp: SqaTcpStream<Reply> },
    /// TCP & UDP connections established - sending `Subscribe` query to server over UDP.
    SubscriptionQuerySent { udp: UdpHandle, tcp: SqaTcpStream<Reply>, ver: String },
    /// Connected via UDP & TCP.
    Connected {
        udp: UdpHandle,
        tcp: SqaTcpStream<Reply>,
        last_ping: u64,
        last_pong: u64,
        ver: String
    }
}
use self::PrivateConnectionState::*;
impl PrivateConnectionState {
    fn take(&mut self) -> Self {
        ::std::mem::replace(self, Disconnected)
    }
    pub fn make_public(&self) -> ConnectionState {
        match *self {
            Disconnected => ConnectionState::Disconnected,
            Connected { last_ping, last_pong, ref ver, ref udp, .. } => ConnectionState::Connected {
                addr: udp.addr.clone(),
                ver: ver.clone(),
                last_ping, last_pong
            },
            ref x => {
                let status = match *x {
                    TcpConnecting { .. } => "Establishing TCP connection",
                    VersionQuerySent { .. } => "Performing UDP handshake",
                    SubscriptionQuerySent { .. } => "Subscribing over UDP",
                    _ => unreachable!()
                };
                let addr = match *x {
                    TcpConnecting { ref addr, .. } => addr.clone(),
                    VersionQuerySent { ref udp, .. } => udp.addr.clone(),
                    SubscriptionQuerySent { ref udp, .. } => udp.addr.clone(),
                    _ => unreachable!()
                };
                ConnectionState::Connecting { addr, status }
            }
        }
    }
}
pub enum ConnectionUIMessage {
    ConnectClicked,
    Show,
    NewlyConnected,
    NewlyDisconnected,
    UndoState(UndoState),
    StartupButton(bool)
}
pub enum ConnectionMessage {
    Disconnect,
    Connect(SocketAddr),
    Send(Command),
}
pub struct Context {
    state: PrivateConnectionState,
    messages: Vec<ConnectionMessage>,
    timeout: Option<Timeout>,
}
impl Context {
    pub fn new() -> Self {
        Self {
            state: PrivateConnectionState::Disconnected,
            messages: vec![],
            timeout: None,
        }
    }
    fn notify_state_change(&mut self, args: &mut BackendContextArgs) {
        let public = self.state.make_public();
        if let Connected { .. } = self.state {
            /* avoid spamming logs */
        }
        else {
            debug!("State change: {:?}", public);
        }
        args.send(UIMessage::ConnState(public));
    }
    fn get_sockets(&mut self) -> Option<(&mut SqaTcpStream<Reply>, &mut UdpHandle)> {
        match self.state {
            VersionQuerySent { ref mut tcp, ref mut udp, .. } => Some((tcp, udp)),
            SubscriptionQuerySent { ref mut tcp, ref mut udp, .. } => Some((tcp, udp)),
            Connected { ref mut tcp, ref mut udp, .. } => Some((tcp, udp)),
            _ => None
        }
    }
    fn send(&mut self, cmd: Command) -> errors::Result<()> {
        if let Some((tcp, udp)) = self.get_sockets() {
            let pkt = ::rosc::OscPacket::Message(cmd.clone().into());
            let pkt = ::rosc::encoder::encode(&pkt)?;
            if pkt.len() > UDP_MAX_PACKET_SIZE {
                trace!("--> (TCP) {:?}", cmd);
                tcp.start_send(cmd.into())?;
                tcp.poll_complete()?;
            }
            else {
                trace!("--> (UDP) {:?}", cmd);
                udp.framed.start_send(pkt)?;
                udp.framed.poll_complete()?;
            }
        }
        else {
            bail!("send() called when not connected");
        }
        Ok(())
    }
    fn update_last_ping(&mut self) {
        if let Connected { ref mut last_ping, .. } = self.state {
            *last_ping = time::precise_time_ns();
        }
    }
    fn ping_timeout(&mut self, args: &mut BackendContextArgs) -> errors::Result<()> {
        if let Connected { last_ping, last_pong, .. } = self.state {
            if last_pong < last_ping {
                info!("Disconnected from server due to ping timeout.");
                self.state = Disconnected;
                args.send(ConnectionUIMessage::NewlyDisconnected.into());
                self.notify_state_change(args);
            }
            else {
                self.send(Command::Ping)?;
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
            x @ UpdateOrder {..} |
            x @ ActionReordered {..} |
            x @ ReplyActionList {..} |
            x @ WaveformGenerated {..} => {
                args.send(UIMessage::ActionReply(x));
            },
            UpdateMixerConf { conf } => {
                args.send(UIMessage::UpdatedMixerConf(conf));
            },
            ReplyUndoState { ctx } => {
                args.send(ConnectionUIMessage::UndoState(ctx).into());
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
        trace!("<-- {:?}", msg);
        if self.state.is_version_query_sent() {
            if let Reply::ServerVersion { ver } = msg {
                let addr = {
                    self.get_sockets().unwrap().0.inner.get_ref().local_addr()?
                };
                self.send(Command::SubscribeAndAssociate { addr })?;
                if let VersionQuerySent { tcp, udp } = self.state.take() {
                    self.state = SubscriptionQuerySent { tcp, udp, ver }
                }
                return Ok(true);
            }
        }
        else if self.state.is_subscription_query_sent() {
            if let Reply::Associated { res } = msg {
                if let Err(e) = res {
                    args.send(Message::Error(format!("Associating with server failed: {}", e)).into());
                    self.state.take();
                    return Ok(true);
                }
                let last_ping = time::precise_time_ns();
                let last_pong = time::precise_time_ns();
                if let SubscriptionQuerySent { tcp, udp, ver } = self.state.take() {
                    self.state = Connected { tcp, udp, ver, last_ping, last_pong }
                }
                self.ping_timeout(args)?;
                self.send(Command::GetMixerConf)?;
                self.send(Command::GetUndoState)?;
                self.send(Command::ActionList)?;
                args.send(SaveMessage::NewlyConnected.into());
                args.send(ConnectionUIMessage::NewlyConnected.into());
                return Ok(true);
            }
        }
        else if self.state.is_connected() {
            if let Reply::Pong = msg {
                if let Connected { ref mut last_pong, .. } = self.state {
                    *last_pong = time::precise_time_ns();
                }
                return Ok(true);
            }
            else {
                self.handle_external_nonstateful(msg, args)?;
            }
        }
        Ok(false)
    }
    fn handle_internal(&mut self, msg: ConnectionMessage, args: &mut BackendContextArgs) -> errors::Result<bool> {
        use self::ConnectionMessage::*;
        match msg {
            Disconnect => {
                self.state.take();
                Ok(true)
            },
            Connect(addr) => {
                info!("Connecting to server: {}", addr);
                self.state.take();
                let tcp = TcpStream::connect(&addr, &args.hdl);
                self.state = TcpConnecting { tcp, addr };
                Ok(true)
            },
            Send(cmd) => {
                if self.state.is_connected() {
                    self.send(cmd)?;
                }
                else {
                    args.send(Message::Error("Not connected, but tried to send messages.".into()).into());
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
        if self.state.is_tcp_connecting() {
            if let TcpConnecting { mut tcp, addr } = self.state.take() {
                match tcp.poll() {
                    Ok(Async::Ready(stream)) => {
                        debug!("TCP connection finished - beginning UDP connection");
                        stream.set_keepalive(Some(Duration::new(1, 0)))?;
                        stream.set_nodelay(true)?;
                        let tcp = SqaTcpStream::new(stream);
                        let recv_addr = "127.0.0.1:53001".parse().unwrap();
                        let codec = SqaClientCodec::new(addr);
                        let sock = UdpSocket::bind(&recv_addr, &args.hdl)?;
                        let mut sock = sock.framed(codec);
                        let pkt = ::rosc::OscPacket::Message(Command::Version.into());
                        let pkt = ::rosc::encoder::encode(&pkt)?;
                        sock.start_send(pkt)?;
                        let udp = UdpHandle { framed: sock, addr };
                        self.state = VersionQuerySent { tcp, udp };
                    },
                    Ok(Async::NotReady) => {
                        self.state = TcpConnecting { tcp, addr };
                        return Ok(());
                    },
                    Err(e) => {
                        args.send(Message::Error(format!("Error connecting to server: {}", e)).into());
                        self.state = Disconnected;
                        self.notify_state_change(&mut args);
                        return Ok(());
                    }
                }
            }
        }
        loop {
            let mut data = vec![];
            if let Some((tcp, udp)) = self.get_sockets() {
                if let Async::Ready(Some(res)) = tcp.poll()? {
                    data.push(res);
                }
                if let Async::Ready(Some(res)) = udp.framed.poll()? {
                    data.push(res);
                }
            }
            if data.len() == 0 {
                break;
            }
            for res in data {
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
            }
        }
        if let Some((tcp, udp)) = self.get_sockets() {
            tcp.poll_complete()?;
            udp.framed.poll_complete()?;
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
    mconnect: MenuItem,
    mdisconnect: MenuItem,
    mconnected: MenuItem,
    mping: MenuItem,
    msetdef: CheckMenuItem,
    msetdef_handler: u64
}

impl ConnectionController {
    pub fn new(b: &Builder, win: Window) -> Self {
        let pwin = PropertyWindow::new("Connection Manager");
        let ipe = FallibleEntry::new();
        let connect_btn = Button::new_with_mnemonic("_Connect");
        let disconnect_btn = Button::new_with_mnemonic("_Disconnect");
        pwin.make_modal(Some(&win));
        pwin.append_property("IP address and port", &*ipe);
        pwin.append_close_btn();
        pwin.append_button(&disconnect_btn);
        pwin.append_button(&connect_btn);
        let state = ConnectionState::Disconnected;
        let mut ret = build!(ConnectionController using b
                             with pwin, ipe, connect_btn, disconnect_btn, state
                             default msetdef_handler, tx
                             get mconnect, mconnected, mdisconnect, msetdef, mping, status_btn, status_img, mundo, mredo);
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
        self.mconnect.connect_activate(clone!(tx; |_| {
            tx.send_internal(ConnectionUIMessage::Show);
        }));
        self.status_btn.connect_clicked(clone!(tx; |_| {
            tx.send_internal(ConnectionUIMessage::Show);
        }));
        self.mdisconnect.connect_activate(clone!(tx; |_| {
            tx.send(ConnectionMessage::Disconnect);
        }));
        self.msetdef_handler = self.msetdef.connect_toggled(clone!(tx; |slf| {
            tx.send_internal(ConfigMessage::SetServerCurrent(slf.get_active()));
        }));
        self.mundo.connect_activate(clone!(tx; |_| {
            tx.send(Command::Undo);
        }));
        self.mredo.connect_activate(clone!(tx; |_| {
            tx.send(Command::Redo);
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
            Show => self.pwin.present(),
            NewlyConnected => {
                info!("Newly connected!");
                self.pwin.hide();
                self.tx.as_mut().unwrap()
                    .send_internal(Message::Statusbar("Connected to server.".into()));
            },
            NewlyDisconnected => {
                info!("Newly disconnected");
                self.tx.as_mut().unwrap().
                    send_internal(Message::Statusbar("Disconnected from server.".into()));
                self.pwin.present();
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
            },
            StartupButton(sb) => {
                signal::signal_handler_block(&self.msetdef, self.msetdef_handler);
                self.msetdef.set_active(sb);
                signal::signal_handler_unblock(&self.msetdef, self.msetdef_handler);
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
                self.mconnected.set_label("Disconnected");
                self.mping.set_label("[no ping]");
                self.msetdef.set_sensitive(false);
                self.mdisconnect.set_sensitive(false);
                self.status_img.set_from_stock("gtk-disconnect", IconSize::Button.into());
                self.status_btn.set_label("disconnected");
            },
            Connecting { addr, status } => {
                self.pwin.update_header(
                    "gtk-refresh",
                    "Connecting...",
                    format!("{}...", status)
                );
                self.mconnected.set_label(&format!("Connecting to {}...", addr));
                self.mping.set_label(&format!("{}...", status));
                self.msetdef.set_sensitive(false);
                self.mdisconnect.set_sensitive(false);
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
                self.mconnected.set_label(&format!("Connected to {}", addr));
                self.mping.set_label(&format!("Ping: {}", ping));
                self.msetdef.set_sensitive(true);
                self.mdisconnect.set_sensitive(true);
                self.status_img.set_from_stock("gtk-connect", IconSize::Button.into());
                self.status_btn.set_label(&format!("{} ({})", addr, ping));
                self.tx.as_mut().unwrap()
                    .send_internal(ConfigMessage::NewlyConnected(addr));
            }
        }
    }
}
