use std::ffi::CString;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::os::fd::AsRawFd;

use anyhow::Context;
use socket2::{Domain, InterfaceIndexOrAddress, Protocol, Socket, Type};

const MDNS_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_PORT: u16 = 5353;

/// The mDNS port on one interface, shared with avahi, umdns or HA's zeroconf.
#[derive(Debug)]
pub struct MdnsSocket {
    socket: UdpSocket,
}

impl MdnsSocket {
    pub fn open(iface: &str) -> anyhow::Result<Self> {
        let index = interface_index(iface).with_context(|| format!("interface {iface}"))?;
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

        share_port_on_interface(&socket, iface)?;
        socket
            .bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MDNS_PORT).into())
            .context("bind UDP 5353")?;
        join_mdns_group(&socket, index)?;

        Ok(Self {
            socket: socket.into(),
        })
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.socket.recv_from(buf)
    }

    pub fn send_multicast(&self, bytes: &[u8]) -> io::Result<()> {
        self.socket
            .send_to(bytes, SocketAddrV4::new(MDNS_GROUP, MDNS_PORT))
            .map(drop)
    }
}

/// Shares port 5353 with other responders and receives only what arrives on `iface`.
fn share_port_on_interface(socket: &Socket, iface: &str) -> anyhow::Result<()> {
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?;

    socket
        .bind_device(Some(iface.as_bytes()))
        .context("SO_BINDTODEVICE")?;

    // Otherwise the socket also receives groups joined by other sockets on other interfaces.
    socket.set_multicast_all_v4(false)?;
    Ok(())
}

fn join_mdns_group(socket: &Socket, index: u32) -> anyhow::Result<()> {
    socket
        .join_multicast_v4_n(&MDNS_GROUP, &InterfaceIndexOrAddress::Index(index))
        .context("join 224.0.0.251")?;
    set_multicast_if(socket, index).context("IP_MULTICAST_IF")?;

    socket.set_multicast_ttl_v4(255)?;
    // On so that HA, in the same network namespace as the OCI relay, hears relayed queries.
    socket.set_multicast_loop_v4(true)?;
    Ok(())
}

fn interface_index(iface: &str) -> io::Result<u32> {
    let name =
        CString::new(iface).map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

    // SAFETY: `name` is a valid NUL-terminated string that outlives the call.
    match unsafe { libc::if_nametoindex(name.as_ptr()) } {
        0 => Err(io::Error::last_os_error()),
        index => Ok(index),
    }
}

/// `socket2` sets `IP_MULTICAST_IF` only by address; the interface may have several or none yet.
fn set_multicast_if(socket: &Socket, index: u32) -> io::Result<()> {
    let mreqn = libc::ip_mreqn {
        imr_multiaddr: libc::in_addr { s_addr: 0 },
        imr_address: libc::in_addr { s_addr: 0 },
        // Kernel interface indexes are positive `int`s -> the cast never wraps.
        imr_ifindex: index.cast_signed(),
    };

    // SAFETY: the option value points to a live `ip_mreqn` of the given length.
    let result = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::IPPROTO_IP,
            libc::IP_MULTICAST_IF,
            (&raw const mreqn).cast(),
            libc::socklen_t::try_from(size_of::<libc::ip_mreqn>())
                .expect("ip_mreqn fits socklen_t"),
        )
    };

    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
