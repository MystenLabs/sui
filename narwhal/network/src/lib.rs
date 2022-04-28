// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

mod primary;
mod retry;
mod worker;

pub use crate::{
    primary::{PrimaryNetwork, PrimaryToWorkerNetwork},
    retry::RetryConfig,
    worker::WorkerNetwork,
};

#[derive(Debug)]
#[must_use]
pub struct CancelHandler<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for CancelHandler<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl<T> std::future::Future for CancelHandler<T> {
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
