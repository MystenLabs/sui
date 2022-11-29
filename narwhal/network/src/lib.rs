// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]
#![allow(clippy::async_yields_async)]

pub mod admin;
pub mod anemo_ext;
pub mod connectivity;
pub mod metrics;
mod p2p;
mod retry;
mod traits;

pub use crate::{
    p2p::P2pNetwork,
    retry::RetryConfig,
    traits::{
        PrimaryToPrimaryRpc, PrimaryToWorkerRpc, ReliableNetwork, UnreliableNetwork, WorkerRpc,
    },
};

/// This adapter will make a [`tokio::task::JoinHandle`] abort its handled task when the handle is dropped.
#[derive(Debug)]
#[must_use]
pub struct CancelOnDropHandler<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for CancelOnDropHandler<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl<T> std::future::Future for CancelOnDropHandler<T> {
    type Output = T;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        use futures::future::FutureExt;
        // If the task panics just propagate it up
        self.0.poll_unpin(cx).map(Result::unwrap)
    }
}

pub fn multiaddr_to_address(
    multiaddr: &multiaddr::Multiaddr,
) -> anyhow::Result<anemo::types::Address> {
    use multiaddr::Protocol;
    let mut iter = multiaddr.iter();

    match (iter.next(), iter.next()) {
        (Some(Protocol::Ip4(ipaddr)), Some(Protocol::Tcp(port) | Protocol::Udp(port))) => {
            Ok((ipaddr, port).into())
        }
        (Some(Protocol::Ip6(ipaddr)), Some(Protocol::Tcp(port) | Protocol::Udp(port))) => {
            Ok((ipaddr, port).into())
        }
        (Some(Protocol::Dns(hostname)), Some(Protocol::Tcp(port) | Protocol::Udp(port))) => {
            Ok((hostname.as_ref(), port).into())
        }

        _ => Err(anyhow::anyhow!("invalid address")),
    }
}
