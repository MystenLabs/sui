// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(not(msim))]
mod inner {
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

    fn get_ephemeral_port() -> ::std::io::Result<u16> {
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

    pub fn new_network_address() -> multiaddr::Multiaddr {
        format!("/dns/localhost/tcp/{}/http", get_available_port())
            .parse()
            .unwrap()
    }
}

#[cfg(msim)]
mod inner {
    pub fn get_available_port() -> u16 {
        use std::cell::Cell;

        thread_local! {
            static PORT: Cell<u32> = Cell::new(32768);
        }

        // TODO: This is a bit of a hack - there is nothing to say that other services haven't already
        // used this port for something else, which could cause problems. We should add a way to ask
        // the simulator for an unused port.
        PORT.with(|port| {
            let ret = port.get();
            port.set(ret + 1);
            ret
        })
        .try_into()
        .expect("ran out of ports")
    }

    pub fn new_network_address() -> multiaddr::Multiaddr {
        let ip_addr = sui_simulator::runtime::NodeHandle::current()
            .ip()
            .expect("expected to be called within a simulator node");

        format!("/ip4/{}/tcp/{}/http", ip_addr, get_available_port())
            .parse()
            .unwrap()
    }
}

pub use inner::*;

pub fn available_local_socket_address() -> std::net::SocketAddr {
    format!("127.0.0.1:{}", get_available_port())
        .parse()
        .unwrap()
}
