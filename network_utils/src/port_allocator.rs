// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use std::net::TcpListener;

pub struct PortAllocator {
    next_port: u16,
}

impl PortAllocator {
    pub fn new(starting_port: u16) -> Self {
        Self {
            next_port: starting_port,
        }
    }
    pub fn next_port(&mut self) -> Option<u16> {
        for port in self.next_port..65535 {
            if TcpListener::bind(("127.0.0.1", port)).is_ok() {
                self.next_port = port + 1;
                return Some(port);
            }
        }
        None
    }
}