// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, TcpListener, TcpStream};

/// Return an ephemeral, available port. On unix systems, the port returned will be in the
/// TIME_WAIT state ensuring that the OS won't hand out this port for some grace period.
/// Callers should be able to bind to this port given they use SO_REUSEADDR.
pub fn get_available_port(host: &str) -> u16 {
    const MAX_PORT_RETRIES: u32 = 1000;

    for _ in 0..MAX_PORT_RETRIES {
        if let Ok(port) = get_ephemeral_port(host) {
            return port;
        }
    }

    panic!("Error: could not find an available port");
}

fn get_ephemeral_port(host: &str) -> std::io::Result<u16> {
    // Request a random available port from the OS
    let listener = TcpListener::bind((host, 0))?;
    let addr = listener.local_addr()?;

    // Create and accept a connection (which we'll promptly drop) in order to force the port
    // into the TIME_WAIT state, ensuring that the port will be reserved from some limited
    // amount of time (roughly 60s on some Linux systems)
    let _sender = TcpStream::connect(addr)?;
    let _incoming = listener.accept()?;

    Ok(addr.port())
}

pub fn new_tcp_network_address() -> sui_types::multiaddr::Multiaddr {
    let host = format!("{}", get_local_ip_for_tests());
    format!("/ip4/{}/tcp/{}/http", host, get_available_port(&host))
        .parse()
        .unwrap()
}

pub fn new_udp_network_address() -> sui_types::multiaddr::Multiaddr {
    let host = format!("{}", get_local_ip_for_tests());
    format!("/ip4/{}/udp/{}", host, get_available_port(&host))
        .parse()
        .unwrap()
}

pub fn available_local_socket_address() -> std::net::SocketAddr {
    let host = "127.0.0.1";
    format!("{}:{}", host, get_available_port(host))
        .parse()
        .unwrap()
}

pub fn available_network_socket_address() -> std::net::SocketAddr {
    let host = "127.0.0.1";
    format!("{}:{}", host, get_available_port(host))
        .parse()
        .unwrap()
}

pub fn socket_address_to_udp_multiaddr(
    address: std::net::SocketAddr,
) -> sui_types::multiaddr::Multiaddr {
    match address {
        std::net::SocketAddr::V4(v4) => format!("/ip4/{}/udp/{}", v4.ip(), v4.port()),
        std::net::SocketAddr::V6(v6) => format!("/ip6/{}/udp/{}", v6.ip(), v6.port()),
    }
    .parse()
    .unwrap()
}

#[cfg(msim)]
pub fn get_local_ip_for_tests() -> IpAddr {
    let node = sui_simulator::runtime::NodeHandle::current();
    node.ip().expect("Current node should have an IP")
}

#[cfg(not(msim))]
pub fn get_local_ip_for_tests() -> IpAddr {
    "127.0.0.1".parse().unwrap()
}
