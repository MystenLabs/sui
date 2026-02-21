// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};

fn allocate_tcp_port() -> io::Result<u16> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

pub fn allocate_ports(count: usize) -> io::Result<Vec<u16>> {
    let mut ports = Vec::with_capacity(count);
    while ports.len() < count {
        let candidate = allocate_tcp_port()?;
        if !ports.contains(&candidate) {
            ports.push(candidate);
        }
    }
    Ok(ports)
}
