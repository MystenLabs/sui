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
/// Because every reservation parks a port in TIME_WAIT, a burst of allocations (for example,
/// many tests building validator configs in parallel) can drain the OS's ephemeral port range.
/// When that happens, this waits for earlier reservations to expire instead of failing.
#[cfg(not(msim))]
pub fn get_available_port(host: &str) -> u16 {
    allocate_port(host, || get_ephemeral_port(host), std::thread::sleep)
        .unwrap_or_else(|e| panic!("Error: could not find an available port on {host}: {e}"))
}

/// How many times to retry when the OS hands out a port that turns out to be unusable (for
/// example, because its UDP counterpart is taken). A fresh attempt usually gets a different port,
/// so these retries are immediate.
#[cfg(not(msim))]
const MAX_UNUSABLE_PORT_RETRIES: u32 = 1000;

/// How long to keep waiting once the ephemeral port range is exhausted. Immediate retries cannot
/// help, because ports only come back as reservations leave TIME_WAIT. Linux holds them there for
/// 60s (macOS for 30s), so this outlasts every reservation that was alive when the wait began.
#[cfg(not(msim))]
const PORT_EXHAUSTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

#[cfg(not(msim))]
const PORT_EXHAUSTION_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// Why a single attempt to reserve a port failed.
#[cfg(not(msim))]
#[derive(Debug)]
enum PortAttemptError {
    /// The OS had no ephemeral port left to hand out.
    RangeExhausted(std::io::Error),
    /// The OS handed out a port that could not be reserved.
    Unusable(std::io::Error),
}

#[cfg(not(msim))]
#[derive(Debug, thiserror::Error)]
enum PortAllocationError {
    #[error(
        "the ephemeral port range stayed exhausted for {waited:?} ({last}); too many ports are \
         held in TIME_WAIT, so reduce test parallelism or widen the ephemeral port range"
    )]
    RangeExhausted {
        waited: std::time::Duration,
        last: std::io::Error,
    },
    #[error("{attempts} ports handed out by the OS were unusable ({last})")]
    Unusable { attempts: u32, last: std::io::Error },
}

/// Retry `attempt` until it reserves a port, retrying unusable ports immediately and waiting out
/// an exhausted port range with `sleep`.
#[cfg(not(msim))]
fn allocate_port(
    host: &str,
    mut attempt: impl FnMut() -> Result<u16, PortAttemptError>,
    mut sleep: impl FnMut(std::time::Duration),
) -> Result<u16, PortAllocationError> {
    let mut unusable_attempts = 0;
    // Counts only time spent sleeping, so the budget does not depend on how long attempts take.
    let mut waited = std::time::Duration::ZERO;

    loop {
        match attempt() {
            Ok(port) => return Ok(port),

            Err(PortAttemptError::Unusable(last)) => {
                unusable_attempts += 1;
                if unusable_attempts >= MAX_UNUSABLE_PORT_RETRIES {
                    return Err(PortAllocationError::Unusable {
                        attempts: unusable_attempts,
                        last,
                    });
                }
            }

            Err(PortAttemptError::RangeExhausted(last)) => {
                if waited >= PORT_EXHAUSTION_TIMEOUT {
                    return Err(PortAllocationError::RangeExhausted { waited, last });
                }
                if waited.is_zero() {
                    tracing::warn!(
                        host,
                        error = %last,
                        "Ephemeral port range exhausted; waiting for TIME_WAIT reservations to \
                         expire"
                    );
                }
                sleep(PORT_EXHAUSTION_POLL_INTERVAL);
                waited += PORT_EXHAUSTION_POLL_INTERVAL;
            }
        }
    }
}

#[cfg(not(msim))]
fn get_ephemeral_port(host: &str) -> Result<u16, PortAttemptError> {
    use std::io::ErrorKind;
    use std::net::{TcpListener, TcpStream, UdpSocket};

    // Request a random available port from the OS. Binding to port 0 cannot conflict with a
    // specific socket, so `AddrInUse` here means no port in the range is free (this is how Linux
    // reports exhaustion).
    let listener = TcpListener::bind((host, 0)).map_err(|e| match e.kind() {
        ErrorKind::AddrInUse => PortAttemptError::RangeExhausted(e),
        _ => PortAttemptError::Unusable(e),
    })?;
    let addr = listener.local_addr().map_err(PortAttemptError::Unusable)?;
    let _udp_socket = UdpSocket::bind(addr).map_err(PortAttemptError::Unusable)?;

    // Create and accept a connection (which we'll promptly drop) in order to force the port
    // into the TIME_WAIT state, ensuring that the port will be reserved from some limited
    // amount of time (roughly 60s on some Linux systems). The connecting side needs an
    // ephemeral port of its own, and `AddrNotAvailable` means none is left (this is where macOS,
    // which parks both ends in TIME_WAIT, runs out first).
    let _sender = TcpStream::connect(addr).map_err(|e| match e.kind() {
        ErrorKind::AddrNotAvailable => PortAttemptError::RangeExhausted(e),
        _ => PortAttemptError::Unusable(e),
    })?;
    let _incoming = listener.accept().map_err(PortAttemptError::Unusable)?;

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

#[cfg(all(test, not(msim)))]
mod tests {
    use std::cell::Cell;
    use std::io::ErrorKind;
    use std::net::{TcpListener, UdpSocket};
    use std::time::Duration;

    use super::*;

    fn exhausted() -> PortAttemptError {
        PortAttemptError::RangeExhausted(ErrorKind::AddrInUse.into())
    }

    fn unusable() -> PortAttemptError {
        PortAttemptError::Unusable(ErrorKind::AddrInUse.into())
    }

    #[test]
    fn waits_out_an_exhausted_range() {
        let attempts = Cell::new(0);
        let mut slept = Vec::new();

        let port = allocate_port(
            "127.0.0.1",
            || {
                attempts.set(attempts.get() + 1);
                if attempts.get() <= 3 {
                    Err(exhausted())
                } else {
                    Ok(1234)
                }
            },
            |d| slept.push(d),
        )
        .unwrap();

        assert_eq!(port, 1234);
        assert_eq!(slept, vec![PORT_EXHAUSTION_POLL_INTERVAL; 3]);
    }

    #[test]
    fn gives_up_when_the_range_stays_exhausted() {
        let mut slept = Duration::ZERO;

        let err = allocate_port("127.0.0.1", || Err(exhausted()), |d| slept += d).unwrap_err();

        let PortAllocationError::RangeExhausted { waited, .. } = err else {
            panic!("expected RangeExhausted, got {err:?}");
        };
        assert_eq!(waited, PORT_EXHAUSTION_TIMEOUT);
        assert_eq!(slept, PORT_EXHAUSTION_TIMEOUT);
    }

    #[test]
    fn retries_unusable_ports_without_waiting() {
        let attempts = Cell::new(0);
        let mut slept = Vec::new();

        let port = allocate_port(
            "127.0.0.1",
            || {
                attempts.set(attempts.get() + 1);
                if attempts.get() <= 5 {
                    Err(unusable())
                } else {
                    Ok(1234)
                }
            },
            |d| slept.push(d),
        )
        .unwrap();

        assert_eq!(port, 1234);
        assert!(slept.is_empty());
    }

    #[test]
    fn gives_up_after_too_many_unusable_ports() {
        let attempts = Cell::new(0);
        let mut slept = Vec::new();

        let err = allocate_port(
            "127.0.0.1",
            || {
                attempts.set(attempts.get() + 1);
                Err(unusable())
            },
            |d| slept.push(d),
        )
        .unwrap_err();

        assert!(
            matches!(
                err,
                PortAllocationError::Unusable {
                    attempts: MAX_UNUSABLE_PORT_RETRIES,
                    ..
                }
            ),
            "expected Unusable after {MAX_UNUSABLE_PORT_RETRIES} attempts, got {err:?}"
        );
        assert_eq!(attempts.get(), MAX_UNUSABLE_PORT_RETRIES);
        assert!(slept.is_empty());
    }

    #[test]
    fn exhaustion_budget_survives_interleaved_unusable_ports() {
        let attempts = Cell::new(0);
        let mut slept = Duration::ZERO;

        // Alternate between the two failure modes. Unusable ports must not reset the time
        // already spent waiting, or a flapping range could keep the caller waiting forever.
        let err = allocate_port(
            "127.0.0.1",
            || {
                attempts.set(attempts.get() + 1);
                if attempts.get() % 2 == 0 {
                    Err(unusable())
                } else {
                    Err(exhausted())
                }
            },
            |d| slept += d,
        )
        .unwrap_err();

        assert!(
            matches!(err, PortAllocationError::RangeExhausted { .. }),
            "expected RangeExhausted, got {err:?}"
        );
        assert_eq!(slept, PORT_EXHAUSTION_TIMEOUT);
    }

    #[test]
    fn reserved_port_can_be_bound() {
        let host = localhost_for_testing();
        let port = get_available_port(&host);

        // std sets SO_REUSEADDR on TCP listeners, so the TIME_WAIT reservation does not block the
        // caller that asked for the port. The UDP side was never connected, so it is free too.
        TcpListener::bind((host.as_str(), port)).unwrap();
        UdpSocket::bind((host.as_str(), port)).unwrap();
    }
}
