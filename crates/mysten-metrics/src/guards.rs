// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::IntGauge;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Increments gauge when acquired, decrements when guard drops
pub struct GaugeGuard<'a>(&'a IntGauge);

impl<'a> GaugeGuard<'a> {
    pub fn acquire(g: &'a IntGauge) -> Self {
        g.inc();
        Self(g)
    }
}

impl<'a> Drop for GaugeGuard<'a> {
    fn drop(&mut self) {
        self.0.dec();
    }
}

pub trait GaugeGuardFutureExt: Future + Sized {
    /// Count number of in flight futures running
    fn count_in_flight(self, g: &IntGauge) -> GaugeGuardFuture<Self>;
}

impl<F: Future> GaugeGuardFutureExt for F {
    fn count_in_flight(self, g: &IntGauge) -> GaugeGuardFuture<Self> {
        GaugeGuardFuture {
            f: Box::pin(self),
            _guard: GaugeGuard::acquire(g),
        }
    }
}

pub struct GaugeGuardFuture<'a, F: Sized> {
    f: Pin<Box<F>>,
    _guard: GaugeGuard<'a>,
}

impl<'a, F: Future> Future for GaugeGuardFuture<'a, F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.f.as_mut().poll(cx)
    }
}
