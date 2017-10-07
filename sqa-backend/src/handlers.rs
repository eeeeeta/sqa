//! Abstractions over handling incoming and outgoing requests and replies.

use codec::{Command, Reply, SqaTcpStream, RecvMessage, SendMessage, SendBytes, SendMessageExt, SqaWireCodec};
use futures::stream::Stream;
use futures::sink::Sink;
use futures::{Poll, Async, Future};
use futures::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};
use tokio_core::net::{UdpFramed, UdpSocket, TcpListener, Incoming};
use tokio_core::reactor::{Handle};
use std::net::SocketAddr;
use time::{Duration, SteadyTime};
use std::collections::HashMap;
use rosc::{OscMessage};
use errors::*;

/// The maximum UDP packet size for packets that SQA Backend will send out.
///
/// **FIXME: make this configurable!**
///
/// *The maximum safe UDP payload is 508 bytes. This is a packet size of 576,
/// minus the maximum 60-byte IP header and the 8-byte UDP header. Any UDP
/// payload this size or smaller is guaranteed to be deliverable over IP (though
/// not guaranteed to be delivered).* - [Beejor on
/// StackOverflow](https://stackoverflow.com/questions/1098897/what-is-the-largest-safe-udp-packet-size-on-the-internet)
pub static UDP_MAX_PACKET_SIZE: usize = 508;

/// A client subscribed with UDP.
pub struct UdpClient {
    /// When the client subscribed.
    subscribed_at: SteadyTime,
    /// A TCP connection source address to associate this client with.
    ///
    /// This must correspond to the source address of an active TCP connection
    /// to be useful. (If it doesn't, it will be set to `None` the next time
    /// something attempts to send to it.)
    ///
    /// If this is present, messages sent to this address that are too big for
    /// UDP will be sent to the given address via the currently-open TCP
    /// connection.
    tcp_addr: Option<SocketAddr>
}
/// A client subscribed with TCP.
pub struct TcpClient {
    /// The client's TCP socket.
    sock: SqaTcpStream<Command>,
    /// Whether the client is subscribed.
    subscribed: bool
}
/// Trait that describes a server, essentially.
///
/// All of these methods take place in the context of a `futures` `Task`. You
/// may call `poll()` within them. In that case, the `wakeup()` method will be
/// called when the future you called `poll()` on is ready to move forward.
pub trait ConnHandler {
    /// The server's internal message type.
    type Message;
    /// Called when the server receives an internal message.
    fn internal(&mut self, d: &mut ConnData<Self::Message>, m: Self::Message);
    /// Called when the server receives an external message.
    fn external(&mut self, d: &mut ConnData<Self::Message>, p: Command, r: ReplyData) -> BackendResult<()>;
    /// Called when the server is first instantiated.
    fn init(&mut self, d: &mut ConnData<Self::Message>);
    /// Called when a user-polled future is ready to move forward. (See the
    /// struct-level documentation for more info.)
    fn wakeup(&mut self, d: &mut ConnData<Self::Message>);
}
/// A handle through which internal messages can be sent.
///
/// The `M` type here refers to the `Message` associated type of `ConnHandler`.
pub struct IntSender<M> {
    tx: UnboundedSender<M>,
}
impl<M> Clone for IntSender<M> {
    fn clone(&self) -> Self {
        IntSender {
            tx: self.tx.clone(),
        }
    }
}
impl<M> IntSender<M> where M: Send + 'static {
    /// Send an internal message.
    pub fn send(&self, msg: M) {
        UnboundedSender::send(&self.tx, msg).unwrap();
    }
}
/// Describes the type of request in a `ReplyData` struct.
enum ReplyDataType {
    TcpRequest,
    UdpRequest
}
/// Data about the author of the connection currently being processed.
///
/// Needed for convenience methods like `respond()` on `ConnData`. **Avoid doing
/// sneaky crap** like storing this somewhere and reusing it for another
/// connection. You're really *not allowed* to do that, and trying to do so will
/// result in panicking.
pub struct ReplyData {
    ty: ReplyDataType,
    addr: SocketAddr
}

/// Struct containing connections and such.
pub struct ConnData<M> {
    /// The server's UDP socket.
    pub framed: UdpFramed<SqaWireCodec>,
    /// The server's TCP listener socket.
    pub incoming: Incoming,
    /// The server's internal message receiver.
    pub internal_rx: UnboundedReceiver<M>,
    /// A copy of the server's internal message sender.
    pub int_sender: IntSender<M>,
    /// Current UDP clients (map of source address to client details).
    pub udp_clients: HashMap<SocketAddr, UdpClient>,
    /// Current TCP clients (map of source address to client details).
    pub tcp_clients: HashMap<SocketAddr, TcpClient>,
    /// Event loop handle.
    pub handle: Handle
}
impl<M> ConnData<M> {
    /// Send a (normally UDP, but varies) message to a given address.
    ///
    /// # Transport
    ///
    /// If the message exceeds `UDP_MAX_PACKET_SIZE` in size, and the address
    /// given is registered in `udp_clients` (i.e. has subscribed) with a
    /// corresponding `tcp_addr` that corresponds to an open TCP connection, the
    /// message will be sent via TCP.
    ///
    /// Otherwise, an `OversizedReply` reply will be sent.
    ///
    /// # Failure
    ///
    /// If sending a message to a subscribed TCP client fails, the client will
    /// be unsubscribed.
    pub fn send_raw(&mut self, msg: SendMessage) -> BackendResult<()> {
        trace!("--> {:?}", msg);
        let SendMessage { pkt, addr } = msg.clone();
        let pkt = ::rosc::OscPacket::Message(pkt);
        let mut pkt = ::rosc::encoder::encode(&pkt)?;
        if pkt.len() > UDP_MAX_PACKET_SIZE {
            debug!("Packet length ({}) exceeds maximum packet size.", pkt.len());
            if let Some(udp_cli) = self.udp_clients.get_mut(&addr) {
                if udp_cli.tcp_addr.is_some() {
                    if let Some(tcp_cli) = self.tcp_clients.get_mut(udp_cli.tcp_addr.as_ref().unwrap()) {
                        debug!("Found a working TCP client, using that");
                        tcp_cli.sock.start_send(msg.pkt)?;
                        return Ok(());
                    }
                    else {
                        debug!("UDP client had non-existent TCP client");
                        udp_cli.tcp_addr = None;
                    }
                }
                else {
                    debug!("UDP client has no TCP client configured");
                }
            }
            else {
                debug!("UDP client isn't subscribed");
            }
            pkt = ::rosc::encoder::encode(&::rosc::OscPacket::Message(
                Reply::OversizedReply.into()
            ))?;
        }
        self.framed.start_send(SendBytes { addr, pkt })?;
        Ok(())
    }
    /// Send a message over TCP.
    ///
    /// If there's no TCP connection with the source address `msg.addr`, this
    /// will return an error.
    pub fn send_tcp(&mut self, msg: SendMessage) -> BackendResult<()> {
        if let Some(cli) = self.tcp_clients.get_mut(&msg.addr) {
            cli.sock.start_send(msg.pkt)?;
            Ok(())
        }
        else {
            bail!("No TCP connection with source address {}", msg.addr);
        }
    }
    /// Respond to the author of the message currently being processed.
    pub fn respond<T: Into<OscMessage>>(&mut self, rpldata: &ReplyData, msg: T) -> BackendResult<()> {
        use self::ReplyDataType::*;
        match rpldata.ty {
            UdpRequest => self.send_raw(rpldata.addr.msg_to(msg.into()))?,
            TcpRequest => self.send_tcp(rpldata.addr.msg_to(msg.into()))?
        }
        Ok(())
    }
    /// Add the author of the message currently being processed as a subscribed client.
    pub fn subscribe(&mut self, rpldata: &ReplyData) {
        use self::ReplyDataType::*;
        match rpldata.ty {
            UdpRequest => {
                debug!("UDP client at {} just subscribed", rpldata.addr);
                self.udp_clients.retain(|pad, _| {
                    rpldata.addr != *pad
                });
                self.udp_clients.insert(rpldata.addr.clone(), UdpClient {
                    subscribed_at: SteadyTime::now(),
                    tcp_addr: None
                });
            },
            TcpRequest => {
                let cli = self.tcp_clients.get_mut(&rpldata.addr)
                    .expect("Something's up with ReplyData");
                cli.subscribed = true;
                debug!("TCP client at {} just subscribed", rpldata.addr);
            }
        }
    }
    /// Associate the current (UDP) connection with a corresponding TCP connection.
    pub fn associate(&mut self, rd: &ReplyData, addr: SocketAddr) -> BackendResult<()> {
        if let ReplyDataType::UdpRequest = rd.ty {
            if let Some(cli) = self.udp_clients.get_mut(&rd.addr) {
                if self.tcp_clients.get(&addr).is_some() {
                    debug!("UDP client at {} associated TCP address {}", rd.addr, addr);
                    cli.tcp_addr = Some(addr);
                    return Ok(());
                }
                bail!("Address to associate isn't an active TCP connection source address.");
            }
            bail!("Can't associate a non-subscribed client.");
        }
        bail!("Can't associate on a TCP connection.");
    }
    /// Broadcast something to all currently subscribed clients.
    ///
    /// Returns the number of messages successfully sent.
    pub fn broadcast<T: Into<OscMessage>>(&mut self, pdata: T) -> BackendResult<usize> {
        let mut n_sent = 0;
        let now = SteadyTime::now();
        self.udp_clients.retain(|addr, party| {
            let ret = now - party.subscribed_at <= Duration::seconds(30);
            if !ret {
                debug!("UDP client at {} hasn't sent anything for 30s. Unsubscribing.", addr);
            }
            ret
        });
        let data = pdata.into();
        let mut udp = Vec::with_capacity(self.udp_clients.len());
        let mut tcp = Vec::with_capacity(self.tcp_clients.len());
        for (addr, _) in self.udp_clients.iter() {
            udp.push(addr.clone());
            n_sent += 1;
        }
        for (addr, cli) in self.tcp_clients.iter() {
            if cli.subscribed {
                tcp.push(addr.clone());
            }
        }
        for addr in udp {
            if let Ok(_) = self.send_raw(addr.msg_to(data.clone())) {
                n_sent += 1;
            }
        }
        for addr in tcp {
            if let Ok(_) = self.send_tcp(addr.msg_to(data.clone())) {
                n_sent += 1;
            }
        }
        Ok(n_sent)
    }
}
pub struct Connection<H> where H: ConnHandler {
    hdlr: H,
    data: ConnData<H::Message>
}
impl<H> Connection<H> where H: ConnHandler {
    fn on_external(&mut self, pkt: BackendResult<Command>, rd: ReplyData) -> BackendResult<()> {
        match pkt {
            Ok(pkt) => {
                if let ReplyDataType::UdpRequest = rd.ty {
                    for (addr, party) in self.data.udp_clients.iter_mut() {
                        if addr == addr {
                            party.subscribed_at = SteadyTime::now();
                        }
                    }
                }
                if let Err(e) = self.hdlr.external(&mut self.data, pkt, rd) {
                    error!("in external handler: {:?}", e);
                }
            },
            Err(e) => {
                let msg = rd.addr.msg_to(
                    Reply::DeserFailed { err: e.to_string() }.into()
                );
                match rd.ty {
                    ReplyDataType::UdpRequest => self.data.send_raw(msg)?,
                    ReplyDataType::TcpRequest => self.data.send_tcp(msg)?
                }
                warn!("Deser failed: {:?}", e);
            }
        };
        Ok(())
    }
}
impl<H> Future for Connection<H> where H: ConnHandler {
    type Item = ();
    type Error = BackendError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        'outer: loop {
            match self.data.internal_rx.poll() {
                Ok(Async::Ready(msg)) => {
                    self.hdlr.internal(&mut self.data, msg.unwrap());
                },
                Ok(Async::NotReady) => {
                    match self.data.framed.poll()? {
                        Async::Ready(Some(RecvMessage { addr, pkt })) => {
                            let rd = ReplyData {
                                ty: ReplyDataType::UdpRequest,
                                addr
                            };
                            if let Err(e) = self.on_external(pkt.map(|x| x.1), rd) {
                                error!("error in external handler: {:?}", e);
                            }
                        },
                        Async::Ready(None) => unreachable!(),
                        Async::NotReady => {
                            let mut msg = None;
                            let mut to_remove = vec![];
                            for (addr, cli) in self.data.tcp_clients.iter_mut() {
                                match cli.sock.poll() {
                                    Ok(Async::Ready(Some(pkt))) => {
                                        msg = Some((pkt, ReplyData {
                                            ty: ReplyDataType::TcpRequest,
                                            addr: *addr,
                                        }));
                                        break;
                                    },
                                    Ok(Async::Ready(None)) => {
                                        debug!("TCP client at {} disconnected", addr);
                                        to_remove.push(*addr);
                                    },
                                    Err(e) => {
                                        debug!("TCP client at {} errored: {:?}", addr, e);
                                        to_remove.push(*addr);
                                    },
                                    _ => {}
                                }
                            }
                            self.data.tcp_clients.retain(|addr, _| {
                                !to_remove.contains(&addr)
                            });
                            match msg {
                                Some((pkt, rd)) => {
                                    if let Err(e) = self.on_external(pkt, rd) {
                                        error!("error in external handler: {}", e);
                                    }
                                },
                                None => {
                                    match self.data.incoming.poll()? {
                                        Async::Ready(Some((sock, addr))) => {
                                            debug!("TCP client at {} connected", addr);
                                            sock.set_keepalive(Some(::std::time::Duration::new(5, 0)))?;
                                            sock.set_nodelay(true)?;
                                            let sock = SqaTcpStream::new(sock);
                                            self.data.tcp_clients.insert(addr, TcpClient {
                                                sock,
                                                subscribed: false
                                            });
                                        },
                                        Async::Ready(None) => unreachable!(),
                                        Async::NotReady => {
                                            self.hdlr.wakeup(&mut self.data);
                                            break 'outer;
                                        }
                                    }
                                }
                            }
                        },
                    }
                },
                Err(_) => {}
            }
        }
        self.data.framed.poll_complete()?;
        for (_, cli) in self.data.tcp_clients.iter_mut() {
            cli.sock.poll_complete()?;
        }
        Ok(Async::NotReady)
    }
}
impl<H> Connection<H> where H: ConnHandler {
    pub fn new(socket: UdpSocket, tcp: TcpListener, handle: Handle, handler: H) -> Self {
        let framed = socket.framed(SqaWireCodec);
        let incoming = tcp.incoming();
        let (tx, rx) = mpsc::unbounded::<H::Message>();
        let is = IntSender {
            tx: tx.clone(),
        };
        let mut ret = Connection {
            data: ConnData {
                framed,
                incoming,
                internal_rx: rx,
                tcp_clients: HashMap::new(),
                udp_clients: HashMap::new(),
                handle: handle,
                int_sender: is
            },
            hdlr: handler
        };
        ret.hdlr.init(&mut ret.data);
        ret
    }
    pub fn get_int_sender(&self) -> IntSender<H::Message> {
        self.data.int_sender.clone()
    }
}
