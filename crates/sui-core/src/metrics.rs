// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use pin_project::{pin_project, pinned_drop};
use prometheus::IntGauge;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

// TODO - this eventually needs to be moved to infra crate if proven useful
#[pin_project(PinnedDrop)]
pub struct Watchdog<W> {
    #[pin]
    inner: W,
    gauge: IntGauge,
}

impl<W> Watchdog<W> {
    pub fn new(gauge: IntGauge, inner: W) -> Self {
        gauge.inc();
        Self { inner, gauge }
    }
}

impl<W: Future> Future for Watchdog<W> {
    type Output = W::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        this.inner.poll(cx)
    }
}

#[pinned_drop]
impl<W> PinnedDrop for Watchdog<W> {
    fn drop(self: Pin<&mut Self>) {
        self.gauge.dec();
    }
}

pub trait WatchdogFutureExt: Future + Sized {
    fn watch_pending(self, gauge: IntGauge) -> Watchdog<Self> {
        Watchdog::new(gauge, self)
    }
}

impl<T: Future> WatchdogFutureExt for T {}
