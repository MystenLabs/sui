// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]
#![allow(clippy::async_yields_async)]

mod bounded_executor;
pub mod metrics;
mod primary;
mod retry;
mod traits;
mod worker;

pub use crate::{
    bounded_executor::BoundedExecutor,
    primary::{P2pNetwork, PrimaryToWorkerNetwork},
    retry::RetryConfig,
    traits::{
        Lucky, LuckyNetwork, LuckyNetwork2, ReliableNetwork, ReliableNetwork2, UnreliableNetwork,
        UnreliableNetwork2,
    },
    worker::{WorkerNetwork, WorkerToPrimaryNetwork},
};

// the result of our network messages
pub type MessageResult = Result<tonic::Response<types::Empty>, eyre::Report>;

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

// This is the maximum number of network tasks that we will create for sending messages. It is a
// limit per network struct - PrimaryNetwork, PrimaryToWorkerNetwork, and WorkerNetwork each have
// their own limit.
//
// The exact number here probably isn't important, the key things is that it should be finite so
// that we don't create unbounded numbers of tasks.
pub const MAX_TASK_CONCURRENCY: usize = 500;

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
