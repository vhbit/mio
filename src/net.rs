//! Networking primitives
//!
use std::fmt;
use std::str::FromStr;
use std::old_io::net::ip::SocketAddr as StdSocketAddr;
use std::old_io::net::ip::ParseError;
use io::{IoHandle, NonBlock};
use error::MioResult;
use buf::{Buf, MutBuf};
use os;

pub use std::old_io::net::ip::{IpAddr, Port};
pub use std::old_io::net::ip::Ipv4Addr as IPv4Addr;
pub use std::old_io::net::ip::Ipv6Addr as IPv6Addr;

use self::SockAddr::{InetAddr,UnixAddr};
use self::AddressFamily::{Unix,Inet,Inet6};

pub trait Socket : IoHandle {
    fn linger(&self) -> MioResult<usize> {
        os::linger(self.desc())
    }

    fn set_linger(&self, dur_s: usize) -> MioResult<()> {
        os::set_linger(self.desc(), dur_s)
    }

    fn set_reuseaddr(&self, val: bool) -> MioResult<()> {
        os::set_reuseaddr(self.desc(), val)
    }

    fn set_reuseport(&self, val: bool) -> MioResult<()> {
        os::set_reuseport(self.desc(), val)
    }
}

pub trait MulticastSocket : Socket {
    fn join_multicast_group(&self, addr: &IpAddr, interface: &Option<IpAddr>) -> MioResult<()> {
        os::join_multicast_group(self.desc(), addr, interface)
    }

    fn leave_multicast_group(&self, addr: &IpAddr, interface: &Option<IpAddr>) -> MioResult<()> {
        os::leave_multicast_group(self.desc(), addr, interface)
    }

    fn set_multicast_ttl(&self, val: u8) -> MioResult<()> {
        os::set_multicast_ttl(self.desc(), val)
    }
}

pub trait UnconnectedSocket {
    fn send_to<B: Buf>(&mut self, buf: &mut B, tgt: &SockAddr) -> MioResult<NonBlock<()>>;

    fn recv_from<B: MutBuf>(&mut self, buf: &mut B) -> MioResult<NonBlock<SockAddr>>;
}

// Types of sockets
#[derive(Copy)]
pub enum AddressFamily {
    Inet,
    Inet6,
    Unix,
}

pub enum SockAddr {
    UnixAddr(Path),
    InetAddr(IpAddr, Port)
}

impl SockAddr {
    pub fn parse(s: &str) -> Result<SockAddr, ParseError> {
        let addr = FromStr::from_str(s);
        addr.map(|a : StdSocketAddr| InetAddr(a.ip, a.port))
    }

    pub fn family(&self) -> AddressFamily {
        match *self {
            UnixAddr(..) => Unix,
            InetAddr(IPv4Addr(..), _) => Inet,
            InetAddr(IPv6Addr(..), _) => Inet6
        }
    }

    pub fn from_path(p: Path) -> SockAddr {
        UnixAddr(p)
    }

    #[inline]
    pub fn consume_std(addr: StdSocketAddr) -> SockAddr {
        InetAddr(addr.ip, addr.port)
    }

    #[inline]
    pub fn from_std(addr: &StdSocketAddr) -> SockAddr {
        InetAddr(addr.ip.clone(), addr.port)
    }

    pub fn to_std(&self) -> Option<StdSocketAddr> {
        match *self {
            InetAddr(ref addr, port) => Some(StdSocketAddr {
                ip: addr.clone(),
                port: port
            }),
            _ => None
        }
    }

    pub fn into_std(self) -> Option<StdSocketAddr> {
        match self {
            InetAddr(addr, port) => Some(StdSocketAddr {
                ip: addr,
                port: port
            }),
            _ => None
        }
    }
}

impl FromStr for SockAddr {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<SockAddr, ParseError> {
        SockAddr::parse(s)
    }
}

impl fmt::Debug for SockAddr {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            InetAddr(ip, port) => write!(fmt, "{}:{}", ip, port),
            _ => write!(fmt, "not implemented")
        }
    }
}

#[derive(Copy)]
pub enum SocketType {
    Dgram,
    Stream,
}

/// TCP networking primitives
///
pub mod tcp {
    use os;
    use error::MioResult;
    use buf::{Buf, MutBuf};
    use io;
    use io::{IoHandle, IoAcceptor, IoReader, IoWriter, NonBlock};
    use io::NonBlock::{Ready, WouldBlock};
    use net::{Socket, SockAddr};
    use net::SocketType::Stream;
    use net::AddressFamily::{self, Inet, Inet6};

    #[derive(Debug)]
    pub struct TcpSocket {
        desc: os::IoDesc
    }

    impl TcpSocket {
        pub fn v4() -> MioResult<TcpSocket> {
            TcpSocket::new(Inet)
        }

        pub fn v6() -> MioResult<TcpSocket> {
            TcpSocket::new(Inet6)
        }

        fn new(family: AddressFamily) -> MioResult<TcpSocket> {
            Ok(TcpSocket { desc: try!(os::socket(family, Stream)) })
        }

        /// Connects the socket to the specified address. When the operation
        /// completes, the handler will be notified with the supplied token.
        ///
        /// The goal of this method is to ensure that the event loop will always
        /// notify about the connection, even if the connection happens
        /// immediately. Otherwise, every consumer of the event loop would have
        /// to worry about possibly-immediate connection.
        pub fn connect(&self, addr: &SockAddr) -> MioResult<()> {
            debug!("socket connect; addr={:?}", addr);

            // Attempt establishing the context. This may not complete immediately.
            if try!(os::connect(&self.desc, addr)) {
                // On some OSs, connecting to localhost succeeds immediately. In
                // this case, queue the writable callback for execution during the
                // next event loop tick.
                debug!("socket connected immediately; addr={:?}", addr);
            }

            Ok(())
        }

        pub fn bind(self, addr: &SockAddr) -> MioResult<TcpListener> {
            try!(os::bind(&self.desc, addr));
            Ok(TcpListener { desc: self.desc })
        }

        pub fn getpeername(&self) -> MioResult<SockAddr> {
            os::getpeername(&self.desc)
        }

        pub fn getsockname(&self) -> MioResult<SockAddr> {
            os::getsockname(&self.desc)
        }
    }

    impl IoHandle for TcpSocket {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl IoReader for TcpSocket {
        fn read<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
            io::read(self, buf)
        }

        fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
            io::read_slice(self, buf)
        }
    }

    impl IoWriter for TcpSocket {
        fn write<B: Buf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
            io::write(self, buf)
        }

        fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
            io::write_slice(self, buf)
        }
    }

    impl Socket for TcpSocket {
    }

    #[derive(Debug)]
    pub struct TcpListener {
        desc: os::IoDesc,
    }

    impl TcpListener {
        pub fn listen(self, backlog: usize) -> MioResult<TcpAcceptor> {
            try!(os::listen(self.desc(), backlog));
            Ok(TcpAcceptor { desc: self.desc })
        }
    }

    impl IoHandle for TcpListener {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    #[derive(Debug)]
    pub struct TcpAcceptor {
        desc: os::IoDesc,
    }

    impl TcpAcceptor {
        pub fn new(addr: &SockAddr, backlog: usize) -> MioResult<TcpAcceptor> {
            let sock = try!(TcpSocket::new(addr.family()));
            let listener = try!(sock.bind(addr));
            listener.listen(backlog)
        }
    }

    impl IoHandle for TcpAcceptor {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl Socket for TcpAcceptor {
    }

    impl IoAcceptor for TcpAcceptor {
        type Output = TcpSocket;

        fn accept(&mut self) -> MioResult<NonBlock<TcpSocket>> {
            match os::accept(self.desc()) {
                Ok(sock) => Ok(Ready(TcpSocket { desc: sock })),
                Err(e) => {
                    if e.is_would_block() {
                        return Ok(WouldBlock);
                    }

                    return Err(e);
                }
            }
        }
    }
}

pub mod udp {
    use os;
    use error::MioResult;
    use buf::{Buf, MutBuf};
    use io::{IoHandle, IoReader, IoWriter, NonBlock};
    use io::NonBlock::{Ready, WouldBlock};
    use io;
    use net::{AddressFamily, Socket, MulticastSocket, SockAddr};
    use net::SocketType::Dgram;
    use net::AddressFamily::{Inet, Inet6};
    use super::UnconnectedSocket;

    #[derive(Debug)]
    pub struct UdpSocket {
        desc: os::IoDesc
    }

    impl UdpSocket {
        pub fn v4() -> MioResult<UdpSocket> {
            UdpSocket::new(Inet)
        }

        pub fn v6() -> MioResult<UdpSocket> {
            UdpSocket::new(Inet6)
        }

        fn new(family: AddressFamily) -> MioResult<UdpSocket> {
            Ok(UdpSocket { desc: try!(os::socket(family, Dgram)) })
        }

        pub fn bind(&self, addr: &SockAddr) -> MioResult<()> {
            try!(os::bind(&self.desc, addr));
            Ok(())
        }

        pub fn connect(&self, addr: &SockAddr) -> MioResult<bool> {
            os::connect(&self.desc, addr)
        }

        pub fn bound(addr: &SockAddr) -> MioResult<UdpSocket> {
            let sock = try!(UdpSocket::new(addr.family()));
            try!(sock.bind(addr));
            Ok(sock)
        }

        pub fn set_broadcast(&self, on: bool) -> MioResult<()> {
            os::set_broadcast(&self.desc, on)
        }
    }

    impl IoHandle for UdpSocket {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl Socket for UdpSocket {
    }

    impl MulticastSocket for UdpSocket {
    }

    impl IoReader for UdpSocket {
        fn read<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
            io::read(self, buf)
        }

        fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
            io::read_slice(self, buf)
        }
    }

    impl IoWriter for UdpSocket {
        fn write<B: Buf>(&self, buf: &mut B) -> MioResult<NonBlock<(usize)>> {
            io::write(self, buf)
        }

        fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
            io::write_slice(self, buf)
        }
    }

    // Unconnected socket sender -- trait unique to sockets
    impl UnconnectedSocket for UdpSocket {
        fn send_to<B: Buf>(&mut self, buf: &mut B, tgt: &SockAddr) -> MioResult<NonBlock<()>> {
            match os::sendto(&self.desc, buf.bytes(), tgt) {
                Ok(cnt) => {
                    buf.advance(cnt);
                    Ok(Ready(()))
                }
                Err(e) => {
                    if e.is_would_block() {
                        Ok(WouldBlock)
                    } else {
                        Err(e)
                    }
                }
            }
        }

        fn recv_from<B: MutBuf>(&mut self, buf: &mut B) -> MioResult<NonBlock<SockAddr>> {
            match os::recvfrom(&self.desc, buf.mut_bytes()) {
                Ok((cnt, saddr)) => {
                    buf.advance(cnt);
                    Ok(Ready(saddr))
                }
                Err(e) => {
                    if e.is_would_block() {
                        Ok(WouldBlock)
                    } else {
                        Err(e)
                    }
                }
            }
        }
    }
}

/// Named pipes
pub mod pipe {
    use os;
    use error::MioResult;
    use buf::{Buf, MutBuf};
    use io;
    use io::{IoHandle, IoAcceptor, IoReader, IoWriter, NonBlock};
    use io::NonBlock::{Ready, WouldBlock};
    use net::{Socket, SockAddr, SocketType};
    use net::SocketType::Stream;
    use net::AddressFamily::Unix;

    #[derive(Debug)]
    pub struct UnixSocket {
        desc: os::IoDesc
    }

    impl UnixSocket {
        pub fn stream() -> MioResult<UnixSocket> {
            UnixSocket::new(Stream)
        }

        fn new(socket_type: SocketType) -> MioResult<UnixSocket> {
            Ok(UnixSocket { desc: try!(os::socket(Unix, socket_type)) })
        }

        pub fn connect(&self, addr: &SockAddr) -> MioResult<()> {
            debug!("socket connect; addr={:?}", addr);

            // Attempt establishing the context. This may not complete immediately.
            if try!(os::connect(&self.desc, addr)) {
                // On some OSs, connecting to localhost succeeds immediately. In
                // this case, queue the writable callback for execution during the
                // next event loop tick.
                debug!("socket connected immediately; addr={:?}", addr);
            }

            Ok(())
        }

        pub fn bind(self, addr: &SockAddr) -> MioResult<UnixListener> {
            try!(os::bind(&self.desc, addr));
            Ok(UnixListener { desc: self.desc })
        }
    }

    impl IoHandle for UnixSocket {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl IoReader for UnixSocket {
        fn read<B: MutBuf>(&self, buf: &mut B) -> MioResult<NonBlock<usize>> {
            io::read(self, buf)
        }

        fn read_slice(&self, buf: &mut[u8]) -> MioResult<NonBlock<usize>> {
            io::read_slice(self, buf)
        }
    }

    impl IoWriter for UnixSocket {
        fn write<B: Buf>(&self, buf: &mut B) -> MioResult<NonBlock<usize>> {
            io::write(self, buf)
        }

        fn write_slice(&self, buf: &[u8]) -> MioResult<NonBlock<usize>> {
            io::write_slice(self, buf)
        }
    }

    impl Socket for UnixSocket {
    }

    #[derive(Debug)]
    pub struct UnixListener {
        desc: os::IoDesc,
    }

    impl UnixListener {
        pub fn listen(self, backlog: usize) -> MioResult<UnixAcceptor> {
            try!(os::listen(self.desc(), backlog));
            Ok(UnixAcceptor { desc: self.desc })
        }
    }

    impl IoHandle for UnixListener {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    #[derive(Debug)]
    pub struct UnixAcceptor {
        desc: os::IoDesc,
    }

    impl UnixAcceptor {
        pub fn new(addr: &SockAddr, backlog: usize) -> MioResult<UnixAcceptor> {
            let sock = try!(UnixSocket::stream());
            let listener = try!(sock.bind(addr));
            listener.listen(backlog)
        }
    }

    impl IoHandle for UnixAcceptor {
        fn desc(&self) -> &os::IoDesc {
            &self.desc
        }
    }

    impl Socket for UnixAcceptor {
    }

    impl IoAcceptor for UnixAcceptor {
        type Output = UnixSocket;

        fn accept(&mut self) -> MioResult<NonBlock<UnixSocket>> {
            match os::accept(self.desc()) {
                Ok(sock) => Ok(Ready(UnixSocket { desc: sock })),
                Err(e) => {
                    if e.is_would_block() {
                        return Ok(WouldBlock);
                    }

                    return Err(e);
                }
            }
        }
    }
}
