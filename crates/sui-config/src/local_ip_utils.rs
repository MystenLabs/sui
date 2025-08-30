// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
#[cfg(msim)]
use std::sync::{atomic::{AtomicI16, Ordering}, Arc};
use sui_types::multiaddr::Multiaddr;
#[cfg(not(msim))]
use tracing::{warn, error};

/// Base IP address used for simulation environment.
const BASE_IP: &str = "10.10.0";
/// Starting port for simulation environment.
const BASE_PORT: i16 = 9000;
/// Maximum IP offset to prevent exceeding valid IP range.
const MAX_IP_OFFSET: i16 = 255;

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
            next_port: AtomicI16::new(BASE_PORT),
        }
    }

    /// Generates the next unique IP address in the format `10.10.0.x`.
    /// Panics if the IP offset exceeds the maximum allowed value (255).
    pub fn get_next_ip(&self) -> String {
        let offset = self
            .next_ip_offset
            .fetch_add(1, Ordering::SeqCst);
        if offset > MAX_IP_OFFSET {
            panic!("IP offset exceeded maximum value of {}", MAX_IP_OFFSET);
        }
        format!("{}.{}", BASE_IP, offset)
    }

    pub fn get_next_available_port(&self) -> u16 {
        self.next_port
            .fetch_add(1, Ordering::SeqCst) as u16
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
    get_available_port_with_retries(host, 1000)
        .unwrap_or_else(|| panic!("Failed to find available port on {} after maximum retries", host))
}

/// Attempts to find an available port with a specified number of retries.
/// Returns `None` if no port is found after the maximum retries.
#[cfg(not(msim))]
pub fn get_available_port_with_retries(host: &str, max_retries: u32) -> Option<u16> {
    use std::time::{Duration, Instant};
    
    if host.is_empty() {
        warn!("Host is empty, cannot find available port");
        return None;
    }
    
    if max_retries == 0 {
        warn!(host = %host, "No retries allowed for finding available port");
        return None;
    }

    let start_time = Instant::now();
    let mut last_error = None;

    for attempt in 0..max_retries {
        match get_ephemeral_port(host) {
            Ok(port) => return Some(port),
            Err(e) => {
                last_error = Some(e);
                if attempt < max_retries - 1 {
                    let backoff_ms = std::cmp::min(1 << attempt, 100); // Cap at 100ms
                    std::thread::sleep(Duration::from_millis(backoff_ms));
                }
            }
        }
    }

    warn!(
        host = %host,
        attempts = max_retries,
        duration = ?start_time.elapsed(),
        error = ?last_error,
        "Failed to find available port after maximum retries"
    );
    None
}

#[cfg(not(msim))]
fn get_ephemeral_port(host: &str) -> std::io::Result<u16> {
    use std::net::{TcpListener, TcpStream};

    // Validate host
    if host.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Host cannot be empty"));
    }

    // Request a random available port from the OS
    let listener = TcpListener::bind((host, 0))?;
    let addr = listener.local_addr()?;

    // Create and accept a connection (which we'll promptly drop) in order to force the port
    // into the TIME_WAIT state, ensuring that the port will be reserved from some limited
    // amount of time (roughly 60s on some Linux systems)
    let _sender = TcpStream::connect(addr).map_err(|e| {
        error!(host = %host, port = addr.port(), error = %e, "Failed to connect to port");
        e
    })?;
    let _incoming = listener.accept().map_err(|e| {
        error!(host = %host, port = addr.port(), error = %e, "Failed to accept connection");
        e
    })?;

    Ok(addr.port())
}

/// Returns a new unique TCP address for the given host, by finding a new available port.
pub fn new_tcp_address_for_testing(host: &str) -> Multiaddr {
    if host.is_empty() {
        panic!("Host cannot be empty");
    }
    format!("/ip4/{}/tcp/{}/http", host, get_available_port(host))
        .parse()
        .map_err(|e| panic!("Failed to parse TCP Multiaddr for host {}: {}", host, e))
        .unwrap()
}

/// Returns a new unique UDP address for the given host, by finding a new available port.
pub fn new_udp_address_for_testing(host: &str) -> Multiaddr {
    if host.is_empty() {
        panic!("Host cannot be empty");
    }
    format!("/ip4/{}/udp/{}", host, get_available_port(host))
        .parse()
        .map_err(|e| panic!("Failed to parse UDP Multiaddr for host {}: {}", host, e))
        .unwrap()
}

/// Returns a new unique TCP address in String format for localhost, by finding a new available port on localhost.
pub fn new_local_tcp_socket_for_testing_string() -> String {
    let localhost = localhost_for_testing();
    format!("{}:{}", localhost, get_available_port(&localhost))
}

/// Returns a new unique TCP address (SocketAddr) for localhost, by finding a new available port on localhost.
pub fn new_local_tcp_socket_for_testing() -> SocketAddr {
    new_local_tcp_socket_for_testing_string()
        .parse()
        .map_err(|e| panic!("Failed to parse TCP SocketAddr: {}", e))
        .unwrap()
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
    if host.is_empty() {
        panic!("Host cannot be empty");
    }
    format!("/ip4/{}/tcp/{}/http", host, port)
        .parse()
        .map_err(|e| panic!("Failed to parse deterministic TCP Multiaddr for host {}: {}", host, e))
        .unwrap()
}

pub fn new_deterministic_udp_address_for_testing(host: &str, port: u16) -> Multiaddr {
    if host.is_empty() {
        panic!("Host cannot be empty");
    }
    format!("/ip4/{}/udp/{}", host, port)
        .parse()
        .map_err(|e| panic!("Failed to parse deterministic UDP Multiaddr for host {}: {}", host, e))
        .unwrap()
}