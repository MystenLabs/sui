// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use async_trait::async_trait;
use std::future::Future;
// TODO: complete tests - This kinda sorta facades the whole tokio::mpsc::{Sender, Receiver}: without tests, this will be fragile to maintain.
use futures::{FutureExt, Stream, TryFutureExt};
use prometheus::{IntCounter, IntGauge};
use std::task::{Context, Poll};
use tokio::sync::mpsc::{
    self,
    error::{SendError, TryRecvError, TrySendError},
};

#[cfg(test)]
#[path = "tests/metered_channel_tests.rs"]
mod metered_channel_tests;

/// An [`mpsc::Sender`](tokio::sync::mpsc::Sender) with an [`IntGauge`]
/// counting the number of currently queued items.
#[derive(Debug)]
pub struct Sender<T> {
    inner: mpsc::Sender<T>,
    gauge: IntGauge,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            gauge: self.gauge.clone(),
        }
    }
}

/// An [`mpsc::Receiver`](tokio::sync::mpsc::Receiver) with an [`IntGauge`]
/// counting the number of currently queued items.
#[derive(Debug)]
pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
    gauge: IntGauge,
    total: Option<IntCounter>,
}

impl<T> Receiver<T> {
    /// Receives the next value for this receiver.
    /// Decrements the gauge in case of a successful `recv`.
    pub async fn recv(&mut self) -> Option<T> {
        self.inner
            .recv()
            .inspect(|opt| {
                if opt.is_some() {
                    self.gauge.dec();
                    if let Some(total_gauge) = &self.total {
                        total_gauge.inc();
                    }
                }
            })
            .await
    }

    /// Attempts to receive the next value for this receiver.
    /// Decrements the gauge in case of a successful `try_recv`.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.inner.try_recv().map(|val| {
            self.gauge.dec();
            if let Some(total_gauge) = &self.total {
                total_gauge.inc();
            }
            val
        })
    }

    // TODO: facade [`blocking_recv`](tokio::mpsc::Receiver::blocking_recv) under the tokio feature flag "sync"

    /// Closes the receiving half of a channel without dropping it.
    pub fn close(&mut self) {
        self.inner.close()
    }

    /// Polls to receive the next message on this channel.
    /// Decrements the gauge in case of a successful `poll_recv`.
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        match self.inner.poll_recv(cx) {
            res @ Poll::Ready(Some(_)) => {
                self.gauge.dec();
                if let Some(total_gauge) = &self.total {
                    total_gauge.inc();
                }
                res
            }
            s => s,
        }
    }
}

impl<T> Unpin for Receiver<T> {}

/// A newtype for an `mpsc::Permit` which allows us to inject gauge accounting
/// in the case the permit is dropped w/o sending
pub struct Permit<'a, T> {
    permit: Option<mpsc::Permit<'a, T>>,
    gauge_ref: &'a IntGauge,
}

impl<'a, T> Permit<'a, T> {
    pub fn new(permit: mpsc::Permit<'a, T>, gauge_ref: &'a IntGauge) -> Permit<'a, T> {
        Permit {
            permit: Some(permit),
            gauge_ref,
        }
    }

    pub fn send(mut self, value: T) {
        let sender = self.permit.take().expect("Permit invariant violated!");
        sender.send(value);
        // skip the drop logic, see https://github.com/tokio-rs/tokio/blob/a66884a2fb80d1180451706f3c3e006a3fdcb036/tokio/src/sync/mpsc/bounded.rs#L1155-L1163
        std::mem::forget(self);
    }
}

impl<'a, T> Drop for Permit<'a, T> {
    fn drop(&mut self) {
        // in the case the permit is dropped without sending, we still want to decrease the occupancy of the channel
        self.gauge_ref.dec()
    }
}

impl<T> Sender<T> {
    /// Sends a value, waiting until there is capacity.
    /// Increments the gauge in case of a successful `send`.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.inner
            .send(value)
            .inspect_ok(|_| self.gauge.inc())
            .await
    }

    /// Completes when the receiver has dropped.
    pub async fn closed(&self) {
        self.inner.closed().await
    }

    /// Attempts to immediately send a message on this `Sender`
    /// Increments the gauge in case of a successful `try_send`.
    pub fn try_send(&self, message: T) -> Result<(), TrySendError<T>> {
        self.inner
            .try_send(message)
            // remove this unsightly hack once https://github.com/rust-lang/rust/issues/91345 is resolved
            .map(|val| {
                self.gauge.inc();
                val
            })
    }

    // TODO: facade [`send_timeout`](tokio::mpsc::Sender::send_timeout) under the tokio feature flag "time"
    // TODO: facade [`blocking_send`](tokio::mpsc::Sender::blocking_send) under the tokio feature flag "sync"

    /// Checks if the channel has been closed. This happens when the
    /// [`Receiver`] is dropped, or when the [`Receiver::close`] method is
    /// called.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Waits for channel capacity. Once capacity to send one message is
    /// available, it is reserved for the caller.
    /// Increments the gauge in case of a successful `reserve`.
    pub async fn reserve(&self) -> Result<Permit<'_, T>, SendError<()>> {
        self.inner
            .reserve()
            // remove this unsightly hack once https://github.com/rust-lang/rust/issues/91345 is resolved
            .map(|val| {
                val.map(|permit| {
                    self.gauge.inc();
                    Permit::new(permit, &self.gauge)
                })
            })
            .await
    }

    /// Tries to acquire a slot in the channel without waiting for the slot to become
    /// available.
    /// Increments the gauge in case of a successful `try_reserve`.
    pub fn try_reserve(&self) -> Result<Permit<'_, T>, TrySendError<()>> {
        self.inner.try_reserve().map(|val| {
            // remove this unsightly hack once https://github.com/rust-lang/rust/issues/91345 is resolved
            self.gauge.inc();
            Permit::new(val, &self.gauge)
        })
    }

    // TODO: consider exposing the _owned methods

    // Note: not exposing `same_channel`, as it is hard to implement with callers able to
    // break the coupling between channel and gauge using `gauge`.

    /// Returns the current capacity of the channel.
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    // We're voluntarily not putting WeakSender under a facade.

    /// Returns a reference to the underlying gauge.
    pub fn gauge(&self) -> &IntGauge {
        &self.gauge
    }
}

////////////////////////////////
/// Stream API Wrappers!
////////////////////////////////

/// A wrapper around [`crate::metered_channel::Receiver`] that implements [`Stream`].
///
#[derive(Debug)]
pub struct ReceiverStream<T> {
    inner: Receiver<T>,
}

impl<T> ReceiverStream<T> {
    /// Create a new `ReceiverStream`.
    pub fn new(recv: Receiver<T>) -> Self {
        Self { inner: recv }
    }

    /// Get back the inner `Receiver`.
    pub fn into_inner(self) -> Receiver<T> {
        self.inner
    }

    /// Closes the receiving half of a channel without dropping it.
    pub fn close(&mut self) {
        self.inner.close()
    }
}

impl<T> Stream for ReceiverStream<T> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.inner.poll_recv(cx)
    }
}

impl<T> AsRef<Receiver<T>> for ReceiverStream<T> {
    fn as_ref(&self) -> &Receiver<T> {
        &self.inner
    }
}

impl<T> AsMut<Receiver<T>> for ReceiverStream<T> {
    fn as_mut(&mut self) -> &mut Receiver<T> {
        &mut self.inner
    }
}

impl<T> From<Receiver<T>> for ReceiverStream<T> {
    fn from(recv: Receiver<T>) -> Self {
        Self::new(recv)
    }
}

// TODO: facade PollSender

////////////////////////////////////////////////////////////////
/// Constructor
////////////////////////////////////////////////////////////////

/// Similar to `mpsc::channel`, `channel` creates a pair of `Sender` and `Receiver`
#[track_caller]
pub fn channel<T>(size: usize, gauge: &IntGauge) -> (Sender<T>, Receiver<T>) {
    gauge.set(0);
    let (sender, receiver) = mpsc::channel(size);
    (
        Sender {
            inner: sender,
            gauge: gauge.clone(),
        },
        Receiver {
            inner: receiver,
            gauge: gauge.clone(),
            total: None,
        },
    )
}

#[track_caller]
pub fn channel_with_total<T>(
    size: usize,
    gauge: &IntGauge,
    total_gauge: &IntCounter,
) -> (Sender<T>, Receiver<T>) {
    gauge.set(0);
    let (sender, receiver) = mpsc::channel(size);
    (
        Sender {
            inner: sender,
            gauge: gauge.clone(),
        },
        Receiver {
            inner: receiver,
            gauge: gauge.clone(),
            total: Some(total_gauge.clone()),
        },
    )
}

#[async_trait]
pub trait WithPermit<T> {
    async fn with_permit<F: Future + Send>(&self, f: F) -> Option<(Permit<T>, F::Output)>;
}

#[async_trait]
impl<T: Send> WithPermit<T> for Sender<T> {
    async fn with_permit<F: Future + Send>(&self, f: F) -> Option<(Permit<T>, F::Output)> {
        let permit = self.reserve().await.ok()?;
        Some((permit, f.await))
    }
}
