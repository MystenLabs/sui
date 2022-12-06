// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{TcpListener, TcpStream};

/// Return an ephemeral, available port. On unix systems, the port returned will be in the
/// TIME_WAIT state ensuring that the OS won't hand out this port for some grace period.
/// Callers should be able to bind to this port given they use SO_REUSEADDR.
pub fn get_available_port() -> u16 {
    const MAX_PORT_RETRIES: u32 = 1000;

    for _ in 0..MAX_PORT_RETRIES {
        if let Ok(port) = get_ephemeral_port() {
            return port;
        }
    }

    panic!("Error: could not find an available port");
}

fn get_ephemeral_port() -> std::io::Result<u16> {
    // Request a random available port from the OS
    let listener = TcpListener::bind(("localhost", 0))?;
    let addr = listener.local_addr()?;

    // Create and accept a connection (which we'll promptly drop) in order to force the port
    // into the TIME_WAIT state, ensuring that the port will be reserved from some limited
    // amount of time (roughly 60s on some Linux systems)
    let _sender = TcpStream::connect(addr)?;
    let _incoming = listener.accept()?;

    Ok(addr.port())
}

pub fn new_tcp_network_address() -> multiaddr::Multiaddr {
    format!("/ip4/127.0.0.1/tcp/{}/http", get_available_port())
        .parse()
        .unwrap()
}

pub fn new_udp_network_address() -> multiaddr::Multiaddr {
    format!("/ip4/127.0.0.1/udp/{}", get_available_port())
        .parse()
        .unwrap()
}

pub fn available_local_socket_address() -> std::net::SocketAddr {
    format!("127.0.0.1:{}", get_available_port())
        .parse()
        .unwrap()
}

pub fn udp_multiaddr_to_listen_address(
    multiaddr: &multiaddr::Multiaddr,
) -> Option<std::net::SocketAddr> {
    use multiaddr::Protocol;
    let mut iter = multiaddr.iter();

    match (iter.next(), iter.next()) {
        (Some(Protocol::Ip4(ipaddr)), Some(Protocol::Udp(port))) => Some((ipaddr, port).into()),
        (Some(Protocol::Ip6(ipaddr)), Some(Protocol::Udp(port))) => Some((ipaddr, port).into()),

        (Some(Protocol::Dns(_)), Some(Protocol::Udp(port))) => {
            Some((std::net::Ipv4Addr::UNSPECIFIED, port).into())
        }

        _ => None,
    }
}

pub fn socket_address_to_udp_multiaddr(address: std::net::SocketAddr) -> multiaddr::Multiaddr {
    match address {
        std::net::SocketAddr::V4(v4) => format!("/ip4/{}/udp/{}", v4.ip(), v4.port()),
        std::net::SocketAddr::V6(v6) => format!("/ip6/{}/udp/{}", v6.ip(), v6.port()),
    }
    .parse()
    .unwrap()
}
