// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Provides wrappers to tokio mpsc channels, with metrics on total items sent, received and inflight.

use std::task::{Context, Poll};

use futures::{Future, TryFutureExt as _};
use prometheus::IntGauge;
use tap::Tap;
use tokio::sync::mpsc::{
    self,
    error::{SendError, TryRecvError, TrySendError},
};

use crate::get_metrics;

/// Wraps an [`mpsc::Sender`] with gauges counting the sent and inflight items.
#[derive(Debug)]
pub struct Sender<T> {
    inner: mpsc::Sender<T>,
    inflight: IntGauge,
    sent: IntGauge,
}

impl<T> Sender<T> {
    /// Sends a value, waiting until there is capacity.
    /// Increments the gauge in case of a successful `send`.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.inner
            .send(value)
            .inspect_ok(|_| {
                self.inflight.inc();
                self.sent.inc();
            })
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
                self.inflight.inc();
                self.sent.inc();
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
        self.inner.reserve().await.map(|permit| {
            self.inflight.inc();
            Permit::new(permit, &self.inflight)
        })
    }

    /// Tries to acquire a slot in the channel without waiting for the slot to become
    /// available.
    /// Increments the gauge in case of a successful `try_reserve`.
    pub fn try_reserve(&self) -> Result<Permit<'_, T>, TrySendError<()>> {
        self.inner.try_reserve().map(|val| {
            self.inflight.inc();
            Permit::new(val, &self.inflight)
        })
    }

    // TODO: consider exposing the _owned methods

    /// Returns the current capacity of the channel.
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Returns a reference to the underlying inflight gauge.
    pub fn inflight(&self) -> &IntGauge {
        &self.inflight
    }

    pub fn downgrade(&self) -> WeakSender<T> {
        let sender = self.inner.downgrade();
        WeakSender {
            inner: sender,
            inflight: self.inflight.clone(),
            sent: self.sent.clone(),
        }
    }
}

// Derive Clone manually to avoid the `T: Clone` bound
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            inflight: self.inflight.clone(),
            sent: self.sent.clone(),
        }
    }
}

/// A newtype for an `mpsc::Permit` which allows us to inject gauge accounting
/// in the case the permit is dropped w/o sending
pub struct Permit<'a, T> {
    permit: Option<mpsc::Permit<'a, T>>,
    inflight_ref: &'a IntGauge,
}

impl<'a, T> Permit<'a, T> {
    pub fn new(permit: mpsc::Permit<'a, T>, inflight_ref: &'a IntGauge) -> Permit<'a, T> {
        Permit {
            permit: Some(permit),
            inflight_ref,
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
        self.inflight_ref.dec()
    }
}

#[async_trait::async_trait]
pub trait WithPermit<T> {
    async fn with_permit<F: Future + Send>(&self, f: F) -> Option<(Permit<T>, F::Output)>
    where
        T: 'static;
}

#[async_trait::async_trait]
impl<T: Send> WithPermit<T> for Sender<T> {
    async fn with_permit<F: Future + Send>(&self, f: F) -> Option<(Permit<T>, F::Output)> {
        let permit = self.reserve().await.ok()?;
        Some((permit, f.await))
    }
}

/// Wraps an [`mpsc::WeakSender`] with gauges counting the sent and inflight items.
#[derive(Debug)]
pub struct WeakSender<T> {
    inner: mpsc::WeakSender<T>,
    inflight: IntGauge,
    sent: IntGauge,
}

impl<T> WeakSender<T> {
    pub fn upgrade(&self) -> Option<Sender<T>> {
        self.inner.upgrade().map(|s| Sender {
            inner: s,
            inflight: self.inflight.clone(),
            sent: self.sent.clone(),
        })
    }
}

// Derive Clone manually to avoid the `T: Clone` bound
impl<T> Clone for WeakSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            inflight: self.inflight.clone(),
            sent: self.sent.clone(),
        }
    }
}

/// Wraps an [`mpsc::Receiver`] with gauges counting the inflight and received items.
#[derive(Debug)]
pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
    inflight: IntGauge,
    received: IntGauge,
}

impl<T> Receiver<T> {
    /// Receives the next value for this receiver.
    /// Decrements the gauge in case of a successful `recv`.
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await.tap(|opt| {
            if opt.is_some() {
                self.inflight.dec();
                self.received.inc();
            }
        })
    }

    /// Attempts to receive the next value for this receiver.
    /// Decrements the gauge in case of a successful `try_recv`.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.inner.try_recv().map(|val| {
            self.inflight.dec();
            self.received.inc();
            val
        })
    }

    pub fn blocking_recv(&mut self) -> Option<T> {
        self.inner.blocking_recv().map(|val| {
            self.inflight.dec();
            self.received.inc();
            val
        })
    }

    /// Closes the receiving half of a channel without dropping it.
    pub fn close(&mut self) {
        self.inner.close()
    }

    /// Polls to receive the next message on this channel.
    /// Decrements the gauge in case of a successful `poll_recv`.
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        match self.inner.poll_recv(cx) {
            res @ Poll::Ready(Some(_)) => {
                self.inflight.dec();
                self.received.inc();
                res
            }
            s => s,
        }
    }
}

impl<T> Unpin for Receiver<T> {}

/// Wraps `mpsc::channel` to create a pair of `Sender` and `Receiver`
pub fn channel<T>(name: &str, size: usize) -> (Sender<T>, Receiver<T>) {
    let metrics = get_metrics().expect("Metrics uninitialized");
    let (sender, receiver) = mpsc::channel(size);
    (
        Sender {
            inner: sender,
            inflight: metrics.channel_inflight.with_label_values(&[name]),
            sent: metrics.channel_sent.with_label_values(&[name]),
        },
        Receiver {
            inner: receiver,
            inflight: metrics.channel_inflight.with_label_values(&[name]),
            received: metrics.channel_received.with_label_values(&[name]),
        },
    )
}

/// Wraps an [`mpsc::UnboundedSender`] with gauges counting the sent and inflight items.
#[derive(Debug)]
pub struct UnboundedSender<T> {
    inner: mpsc::UnboundedSender<T>,
    inflight: IntGauge,
    sent: IntGauge,
}

impl<T> UnboundedSender<T> {
    /// Sends a value, waiting until there is capacity.
    /// Increments the gauge in case of a successful `send`.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.inner.send(value).map(|_| {
            self.inflight.inc();
            self.sent.inc();
        })
    }

    /// Completes when the receiver has dropped.
    pub async fn closed(&self) {
        self.inner.closed().await
    }

    /// Checks if the channel has been closed. This happens when the
    /// [`Receiver`] is dropped, or when the [`Receiver::close`] method is
    /// called.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Returns a reference to the underlying inflight gauge.
    pub fn inflight(&self) -> &IntGauge {
        &self.inflight
    }

    pub fn downgrade(&self) -> WeakUnboundedSender<T> {
        let sender = self.inner.downgrade();
        WeakUnboundedSender {
            inner: sender,
            inflight: self.inflight.clone(),
            sent: self.sent.clone(),
        }
    }
}

// Derive Clone manually to avoid the `T: Clone` bound
impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            inflight: self.inflight.clone(),
            sent: self.sent.clone(),
        }
    }
}

/// Wraps an [`mpsc::WeakUnboundedSender`] with gauges counting the sent and inflight items.
#[derive(Debug)]
pub struct WeakUnboundedSender<T> {
    inner: mpsc::WeakUnboundedSender<T>,
    inflight: IntGauge,
    sent: IntGauge,
}

impl<T> WeakUnboundedSender<T> {
    pub fn upgrade(&self) -> Option<UnboundedSender<T>> {
        self.inner.upgrade().map(|s| UnboundedSender {
            inner: s,
            inflight: self.inflight.clone(),
            sent: self.sent.clone(),
        })
    }
}

// Derive Clone manually to avoid the `T: Clone` bound
impl<T> Clone for WeakUnboundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            inflight: self.inflight.clone(),
            sent: self.sent.clone(),
        }
    }
}

/// Wraps an [`mpsc::UnboundedReceiver`] with gauges counting the inflight and received items.
#[derive(Debug)]
pub struct UnboundedReceiver<T> {
    inner: mpsc::UnboundedReceiver<T>,
    inflight: IntGauge,
    received: IntGauge,
}

impl<T> UnboundedReceiver<T> {
    /// Receives the next value for this receiver.
    /// Decrements the gauge in case of a successful `recv`.
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await.tap(|opt| {
            if opt.is_some() {
                self.inflight.dec();
                self.received.inc();
            }
        })
    }

    /// Attempts to receive the next value for this receiver.
    /// Decrements the gauge in case of a successful `try_recv`.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.inner.try_recv().map(|val| {
            self.inflight.dec();
            self.received.inc();
            val
        })
    }

    pub fn blocking_recv(&mut self) -> Option<T> {
        self.inner.blocking_recv().map(|val| {
            self.inflight.dec();
            self.received.inc();
            val
        })
    }

    /// Closes the receiving half of a channel without dropping it.
    pub fn close(&mut self) {
        self.inner.close()
    }

    /// Polls to receive the next message on this channel.
    /// Decrements the gauge in case of a successful `poll_recv`.
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        match self.inner.poll_recv(cx) {
            res @ Poll::Ready(Some(_)) => {
                self.inflight.dec();
                self.received.inc();
                res
            }
            s => s,
        }
    }
}

impl<T> Unpin for UnboundedReceiver<T> {}

/// Wraps an [`mpsc::UnboundedChannel`] to create a pair of `UnboundedSender` and `UnboundedReceiver`
pub fn unbounded_channel<T>(name: &str) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let metrics = get_metrics().expect("Metrics uninitialized");
    #[allow(clippy::disallowed_methods)]
    let (sender, receiver) = mpsc::unbounded_channel();
    (
        UnboundedSender {
            inner: sender,
            inflight: metrics.channel_inflight.with_label_values(&[name]),
            sent: metrics.channel_sent.with_label_values(&[name]),
        },
        UnboundedReceiver {
            inner: receiver,
            inflight: metrics.channel_inflight.with_label_values(&[name]),
            received: metrics.channel_received.with_label_values(&[name]),
        },
    )
}
