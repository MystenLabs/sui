// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
#[cfg(msim)]
use std::sync::{Arc, atomic::AtomicI16};
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
    // Uses Arc so that we could return a clone of the process-global singleton.
    static SIM_ADDRESS_MANAGER: std::sync::OnceLock<Arc<SimAddressManager>> =
        std::sync::OnceLock::new();
    SIM_ADDRESS_MANAGER
        .get_or_init(|| Arc::new(SimAddressManager::new()))
        .clone()
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
///
/// Because every call holds a port in TIME_WAIT for about 60 seconds (fixed on Linux), many tests
/// running in parallel can exhaust the ephemeral port range. Retrying immediately cannot succeed
/// then, so failed attempts back off and keep retrying until earlier reservations expire.
#[cfg(not(msim))]
pub fn get_available_port(host: &str) -> u16 {
    use std::time::{Duration, Instant};

    // Longer than TIME_WAIT, so every reservation that existed when the range ran out has expired
    // before this gives up.
    const RETRY_DEADLINE: Duration = Duration::from_secs(90);
    const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(10);
    const MAX_RETRY_DELAY: Duration = Duration::from_secs(1);

    let start = Instant::now();
    let mut retry_delay = INITIAL_RETRY_DELAY;
    loop {
        match get_ephemeral_port(host) {
            Ok(port) => return port,
            Err(error) if start.elapsed() >= RETRY_DEADLINE => panic!(
                "Error: could not find an available port on {host} after {:?}: {error:?}",
                start.elapsed()
            ),
            Err(_) => {
                std::thread::sleep(retry_delay);
                retry_delay = (retry_delay * 2).min(MAX_RETRY_DELAY);
            }
        }
    }
}

#[cfg(not(msim))]
fn get_ephemeral_port(host: &str) -> std::io::Result<u16> {
    use std::net::{TcpListener, TcpStream, UdpSocket};

    // Request a random available port from the OS
    let listener = TcpListener::bind((host, 0))?;
    let addr = listener.local_addr()?;
    let _udp_socket = UdpSocket::bind(addr)?;

    // Create and accept a connection (which we'll promptly drop) in order to force the port
    // into the TIME_WAIT state, ensuring that the port will be reserved from some limited
    // amount of time (roughly 60s on some Linux systems)
    let _sender = TcpStream::connect(addr)?;
    let _incoming = listener.accept()?;

    Ok(addr.port())
}

/// Returns a new unique TCP address for the given host, by finding a new available port.
pub fn new_tcp_address_for_testing(host: &str) -> Multiaddr {
    format!("/ip4/{}/tcp/{}/https", host, get_available_port(host))
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
    format!("/ip4/{host}/tcp/{port}/https").parse().unwrap()
}

pub fn new_deterministic_udp_address_for_testing(host: &str, port: u16) -> Multiaddr {
    format!("/ip4/{host}/udp/{port}/https").parse().unwrap()
}

/// Number of consecutive ports in each block returned by [`placeholder_port_block_for_testing`].
pub const PLACEHOLDER_PORT_BLOCK_SIZE: u16 = 10;

/// Returns the first port of a block of [`PLACEHOLDER_PORT_BLOCK_SIZE`] consecutive ports for the
/// validator at `validator_index`. Blocks for different indices never overlap, so validators in one
/// genesis get distinct addresses, but nothing checks or reserves these ports. Use them only for
/// addresses that are never bound, such as the configs of in-memory test authorities: a test that
/// binds one can collide with every other test using the same block.
pub fn placeholder_port_block_for_testing(validator_index: usize) -> u16 {
    const FIRST_PLACEHOLDER_PORT: usize = 20_000;
    let block_size = usize::from(PLACEHOLDER_PORT_BLOCK_SIZE);
    let block_start = FIRST_PLACEHOLDER_PORT + validator_index * block_size;
    assert!(
        block_start + block_size - 1 <= usize::from(u16::MAX),
        "validator index {validator_index} is too large for a placeholder port block"
    );
    block_start as u16
}
