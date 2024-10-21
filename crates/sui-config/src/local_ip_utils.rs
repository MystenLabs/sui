// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
#[cfg(msim)]
use std::sync::{atomic::AtomicI16, Arc};
use sui_types::multiaddr::Multiaddr;

/// A singleton struct to manage IP addresses and ports for simtest.
/// This allows us to generate unique IP addresses and ports for each node in simtest.
#[cfg(msim)]
pub struct SimAddressManager {
    next_ip_offset: AtomicI16,
    next_port: AtomicI16,
}

#[cfg(msim)]
impl SimAddressManager {
    pub fn new() -> Self {
        Self {
            next_ip_offset: AtomicI16::new(1),
            next_port: AtomicI16::new(9000),
        }
    }

    pub fn get_next_ip(&self) -> String {
        let offset = self
            .next_ip_offset
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // If offset ever goes beyond 255, we could use more bytes in the IP.
        assert!(offset <= 255);
        format!("10.10.0.{}", offset)
    }

    pub fn get_next_available_port(&self) -> u16 {
        self.next_port
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst) as u16
    }
}

#[cfg(msim)]
fn get_sim_address_manager() -> Arc<SimAddressManager> {
    thread_local! {
        // Uses Arc so that we could return a clone of the thread local singleton.
        static SIM_ADDRESS_MANAGER: Arc<SimAddressManager> = Arc::new(SimAddressManager::new());
    }
    SIM_ADDRESS_MANAGER.with(|s| s.clone())
}

/// In simtest, we generate a new unique IP each time this function is called.
#[cfg(msim)]
pub fn get_new_ip() -> String {
    get_sim_address_manager().get_next_ip()
}

/// In non-simtest, we always only have one IP address which is localhost.
#[cfg(not(msim))]
pub fn get_new_ip() -> String {
    localhost_for_testing()
}

/// Returns localhost, which is always 127.0.0.1.
pub fn localhost_for_testing() -> String {
    "127.0.0.1".to_string()
}

/// Returns an available port for the given host in simtest.
/// We don't care about host because it's all managed by simulator. Just obtain a unique port.
#[cfg(msim)]
pub fn get_available_port(_host: &str) -> u16 {
    get_sim_address_manager().get_next_available_port()
}

/// Return an ephemeral, available port. On unix systems, the port returned will be in the
/// TIME_WAIT state ensuring that the OS won't hand out this port for some grace period.
/// Callers should be able to bind to this port given they use SO_REUSEADDR.
#[cfg(not(msim))]
pub fn get_available_port(host: &str) -> u16 {
    const MAX_PORT_RETRIES: u32 = 1000;

    for _ in 0..MAX_PORT_RETRIES {
        if let Ok(port) = get_ephemeral_port(host) {
            return port;
        }
    }

    panic!(
        "Error: could not find an available port on {}: {:?}",
        host,
        get_ephemeral_port(host)
    );
}

#[cfg(not(msim))]
fn get_ephemeral_port(host: &str) -> std::io::Result<u16> {
    use std::net::{TcpListener, TcpStream};

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

/// Returns a new unique TCP address for the given host, by finding a new available port.
pub fn new_tcp_address_for_testing(host: &str) -> Multiaddr {
    format!("/ip4/{}/tcp/{}/http", host, get_available_port(host))
        .parse()
        .unwrap()
}

/// Returns a new unique UDP address for the given host, by finding a new available port.
pub fn new_udp_address_for_testing(host: &str) -> Multiaddr {
    format!("/ip4/{}/udp/{}", host, get_available_port(host))
        .parse()
        .unwrap()
}

/// Returns a new unique TCP address in String format for localhost, by finding a new available port on localhost.
pub fn new_local_tcp_socket_for_testing_string() -> String {
    format!(
        "{}:{}",
        localhost_for_testing(),
        get_available_port(&localhost_for_testing())
    )
}

/// Returns a new unique TCP address (SocketAddr) for localhost, by finding a new available port on localhost.
pub fn new_local_tcp_socket_for_testing() -> SocketAddr {
    new_local_tcp_socket_for_testing_string().parse().unwrap()
}

/// Returns a new unique TCP address (Multiaddr) for localhost, by finding a new available port on localhost.
pub fn new_local_tcp_address_for_testing() -> Multiaddr {
    new_tcp_address_for_testing(&localhost_for_testing())
}

/// Returns a new unique UDP address for localhost, by finding a new available port.
pub fn new_local_udp_address_for_testing() -> Multiaddr {
    new_udp_address_for_testing(&localhost_for_testing())
}

pub fn new_deterministic_tcp_address_for_testing(host: &str, port: u16) -> Multiaddr {
    format!("/ip4/{host}/tcp/{port}/http").parse().unwrap()
}

pub fn new_deterministic_udp_address_for_testing(host: &str, port: u16) -> Multiaddr {
    format!("/ip4/{host}/udp/{port}/http").parse().unwrap()
}
