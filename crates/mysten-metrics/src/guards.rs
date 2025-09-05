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

impl Drop for GaugeGuard<'_> {
    fn drop(&mut self) {
        self.0.dec();
    }
}

/// Difference vs GaugeGuard: Stores the gauge by value to avoid borrowing issues.
pub struct InflightGuard(IntGauge);

impl InflightGuard {
    pub fn acquire(g: IntGauge) -> Self {
        g.inc();
        Self(g)
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.0.dec();
    }
}

pub trait InflightGuardFutureExt: Future + Sized {
    /// Count number of in flight futures running
    fn count_in_flight(self, g: IntGauge) -> InflightGuardFuture<Self>;
}

impl<F: Future> InflightGuardFutureExt for F {
    fn count_in_flight(self, g: IntGauge) -> InflightGuardFuture<Self> {
        InflightGuardFuture {
            f: Box::pin(self),
            _guard: InflightGuard::acquire(g),
        }
    }
}

pub struct InflightGuardFuture<F: Sized> {
    f: Pin<Box<F>>,
    _guard: InflightGuard,
}

impl<F: Future> Future for InflightGuardFuture<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.f.as_mut().poll(cx)
    }
}
