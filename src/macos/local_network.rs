use std::{
    collections::BTreeSet,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket},
    os::fd::AsRawFd,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalNetwork {
    #[default]
    Unknown,
    Allowed,
    Blocked,
}

const MDNS_V4: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_V6: Ipv6Addr = Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 0, 0xfb);
const MDNS_PORT: u16 = 5353;
const EMPTY_QUERY: [u8; 12] = [0; 12];

pub fn local_network_access() -> LocalNetwork {
    let Ok(interfaces) = if_addrs::get_if_addrs() else {
        return LocalNetwork::Unknown;
    };
    let mut seen = BTreeSet::new();
    classify(
        interfaces
            .into_iter()
            .filter(|interface| {
                interface.is_oper_up()
                    && !interface.is_loopback()
                    && !interface.is_p2p()
                    && !["awdl", "llw"]
                        .iter()
                        .any(|p| interface.name.starts_with(p))
            })
            .filter_map(|interface| Some((interface.ip(), interface.index?)))
            .filter(|&(address, index)| seen.insert((address.is_ipv4(), index)))
            .map(|(address, index)| send(address, index)),
    )
}

fn send(address: IpAddr, index: u32) -> io::Result<()> {
    let (socket, group) = match address {
        IpAddr::V4(address) => {
            let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
            let interface = libc::in_addr {
                s_addr: u32::from(address).to_be(),
            };
            set_option(&socket, libc::IPPROTO_IP, libc::IP_MULTICAST_IF, &interface)?;
            (socket, SocketAddr::from((MDNS_V4, MDNS_PORT)))
        }
        IpAddr::V6(_) => {
            let socket = UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0))?;
            set_option(&socket, libc::IPPROTO_IPV6, libc::IPV6_MULTICAST_IF, &index)?;
            (
                socket,
                SocketAddrV6::new(MDNS_V6, MDNS_PORT, 0, index).into(),
            )
        }
    };
    socket.set_nonblocking(true)?;
    socket.send_to(&EMPTY_QUERY, group).map(drop)
}

fn set_option<T>(socket: &UdpSocket, level: i32, name: i32, value: &T) -> io::Result<()> {
    let status = unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            level,
            name,
            std::ptr::from_ref(value).cast(),
            size_of::<T>() as libc::socklen_t,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn classify(results: impl IntoIterator<Item = io::Result<()>>) -> LocalNetwork {
    let mut state = LocalNetwork::Unknown;
    for result in results {
        match result {
            Ok(()) => return LocalNetwork::Allowed,
            Err(error) if error.kind() == io::ErrorKind::HostUnreachable => {
                state = LocalNetwork::Blocked;
            }
            Err(_) => {}
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(code: i32) -> io::Result<()> {
        Err(io::Error::from_raw_os_error(code))
    }

    #[test]
    fn host_unreachable_is_blocked_and_other_errors_are_unknown() {
        assert_eq!(
            classify([failed(libc::EHOSTUNREACH)]),
            LocalNetwork::Blocked
        );
        assert_eq!(
            classify([failed(libc::ENETUNREACH), failed(libc::EHOSTUNREACH)]),
            LocalNetwork::Blocked
        );
        assert_eq!(
            classify([failed(libc::ENETUNREACH), failed(libc::EADDRNOTAVAIL)]),
            LocalNetwork::Unknown
        );
        assert_eq!(classify([]), LocalNetwork::Unknown);
    }

    #[test]
    fn one_delivered_send_is_allowed_and_stops_the_probe() {
        assert_eq!(
            classify([failed(libc::EHOSTUNREACH), Ok(())]),
            LocalNetwork::Allowed
        );
        let mut sends = 0;
        let state = classify((0..3).map(|_| {
            sends += 1;
            Ok(())
        }));
        assert_eq!((state, sends), (LocalNetwork::Allowed, 1));
    }
}
