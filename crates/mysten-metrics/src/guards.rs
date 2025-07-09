// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{IntGauge, IntGaugeVec};
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

/// Increments IntGaugeVec with labels when acquired, decrements when guard drops
pub struct IntGaugeVecGuard<'a> {
    gauge: &'a IntGaugeVec,
    labels: Vec<String>,
}

impl<'a> IntGaugeVecGuard<'a> {
    pub fn acquire(gauge: &'a IntGaugeVec, labels: &[&str]) -> Self {
        gauge.with_label_values(labels).inc();
        let labels: Vec<String> = labels.iter().map(|s| s.to_string()).collect();
        Self { gauge, labels }
    }
}

impl Drop for IntGaugeVecGuard<'_> {
    fn drop(&mut self) {
        self.gauge
            .with_label_values(&self.labels.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .dec();
    }
}

pub trait GaugeGuardFutureExt: Future + Sized {
    /// Count number of in flight futures running
    fn count_in_flight(self, g: &IntGauge) -> GaugeGuardFuture<Self>;

    /// Count number of in flight futures running with labeled gauge
    fn count_in_flight_with_labels<'a>(
        self,
        g: &'a IntGaugeVec,
        labels: &[&str],
    ) -> IntGaugeVecGuardFuture<'a, Self>;
}

impl<F: Future> GaugeGuardFutureExt for F {
    fn count_in_flight(self, g: &IntGauge) -> GaugeGuardFuture<Self> {
        GaugeGuardFuture {
            f: Box::pin(self),
            _guard: GaugeGuard::acquire(g),
        }
    }

    fn count_in_flight_with_labels<'a>(
        self,
        g: &'a IntGaugeVec,
        labels: &[&str],
    ) -> IntGaugeVecGuardFuture<'a, Self> {
        IntGaugeVecGuardFuture {
            f: Box::pin(self),
            _guard: IntGaugeVecGuard::acquire(g, labels),
        }
    }
}

pub struct GaugeGuardFuture<'a, F: Sized> {
    f: Pin<Box<F>>,
    _guard: GaugeGuard<'a>,
}

impl<F: Future> Future for GaugeGuardFuture<'_, F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.f.as_mut().poll(cx)
    }
}

pub struct IntGaugeVecGuardFuture<'a, F: Sized> {
    f: Pin<Box<F>>,
    _guard: IntGaugeVecGuard<'a>,
}

impl<F: Future> Future for IntGaugeVecGuardFuture<'_, F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.f.as_mut().poll(cx)
    }
}
