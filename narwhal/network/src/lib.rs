// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

mod error;
mod primary;
mod receiver;
mod reliable_sender;
mod retry;
mod simple_sender;
mod worker;

pub use crate::{
    primary::{PrimaryNetwork, PrimaryToWorkerNetwork},
    receiver::{MessageHandler, Receiver, Writer},
    reliable_sender::{CancelHandler, ReliableSender},
    retry::RetryConfig,
    simple_sender::SimpleSender,
    worker::WorkerNetwork,
};

#[derive(Debug)]
#[must_use]
pub struct CancelHandler2<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for CancelHandler2<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl<T> std::future::Future for CancelHandler2<T> {
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
