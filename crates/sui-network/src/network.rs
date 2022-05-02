// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Debug)]
pub struct NetworkClient {
    base_address: String,
    base_port: u16,
    _buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
}

impl NetworkClient {
    pub fn new(
        base_address: String,
        base_port: u16,
        buffer_size: usize,
        send_timeout: std::time::Duration,
        recv_timeout: std::time::Duration,
    ) -> Self {
        NetworkClient {
            base_address,
            base_port,
            _buffer_size: buffer_size,
            send_timeout,
            recv_timeout,
        }
    }

    pub fn base_address(&self) -> &str {
        &self.base_address
    }

    pub fn base_port(&self) -> u16 {
        self.base_port
    }

    pub fn send_timeout(&self) -> std::time::Duration {
        self.send_timeout
    }

    pub fn recv_timeout(&self) -> std::time::Duration {
        self.recv_timeout
    }
}

pub struct NetworkServer {
    pub base_address: String,
    pub base_port: u16,
    pub buffer_size: usize,
    // Stats
    packets_processed: AtomicUsize,
    user_errors: AtomicUsize,
}

impl NetworkServer {
    pub fn new(base_address: String, base_port: u16, buffer_size: usize) -> Self {
        Self {
            base_address,
            base_port,
            buffer_size,
            packets_processed: AtomicUsize::new(0),
            user_errors: AtomicUsize::new(0),
        }
    }

    pub fn packets_processed(&self) -> usize {
        self.packets_processed.load(Ordering::Relaxed)
    }

    pub fn increment_packets_processed(&self) {
        self.packets_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn user_errors(&self) -> usize {
        self.user_errors.load(Ordering::Relaxed)
    }

    pub fn increment_user_errors(&self) {
        self.user_errors.fetch_add(1, Ordering::Relaxed);
    }
}
