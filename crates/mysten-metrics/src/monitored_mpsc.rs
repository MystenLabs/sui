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

/// Wraps [`mpsc::Sender`] with gauges counting the sent and inflight items.
#[derive(Debug)]
pub struct Sender<T> {
    inner: mpsc::Sender<T>,
    inflight: Option<IntGauge>,
    sent: Option<IntGauge>,
}

impl<T> Sender<T> {
    /// Sends a value, waiting until there is capacity.
    /// Increments the gauge in case of a successful `send`.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.inner
            .send(value)
            .inspect_ok(|_| {
                if let Some(inflight) = &self.inflight {
                    inflight.inc();
                }
                if let Some(sent) = &self.sent {
                    sent.inc();
                }
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
            // TODO: switch to inspect() once the repo upgrades to Rust 1.76 or higher.
            .map(|_| {
                if let Some(inflight) = &self.inflight {
                    inflight.inc();
                }
                if let Some(sent) = &self.sent {
                    sent.inc();
                }
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
            if let Some(inflight) = &self.inflight {
                inflight.inc();
            }
            Permit::new(permit, &self.inflight, &self.sent)
        })
    }

    /// Tries to acquire a slot in the channel without waiting for the slot to become
    /// available.
    /// Increments the gauge in case of a successful `try_reserve`.
    pub fn try_reserve(&self) -> Result<Permit<'_, T>, TrySendError<()>> {
        self.inner.try_reserve().map(|val| {
            if let Some(inflight) = &self.inflight {
                inflight.inc();
            }
            Permit::new(val, &self.inflight, &self.sent)
        })
    }

    // TODO: consider exposing the _owned methods

    /// Returns the current capacity of the channel.
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    pub fn downgrade(&self) -> WeakSender<T> {
        let sender = self.inner.downgrade();
        WeakSender {
            inner: sender,
            inflight: self.inflight.clone(),
            sent: self.sent.clone(),
        }
    }

    /// Returns a reference to the underlying inflight gauge.
    #[cfg(test)]
    fn inflight(&self) -> &IntGauge {
        self.inflight
            .as_ref()
            .expect("Metrics should have initialized")
    }

    /// Returns a reference to the underlying sent gauge.
    #[cfg(test)]
    fn sent(&self) -> &IntGauge {
        self.sent.as_ref().expect("Metrics should have initialized")
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
    inflight_ref: &'a Option<IntGauge>,
    sent_ref: &'a Option<IntGauge>,
}

impl<'a, T> Permit<'a, T> {
    pub fn new(
        permit: mpsc::Permit<'a, T>,
        inflight_ref: &'a Option<IntGauge>,
        sent_ref: &'a Option<IntGauge>,
    ) -> Permit<'a, T> {
        Permit {
            permit: Some(permit),
            inflight_ref,
            sent_ref,
        }
    }

    pub fn send(mut self, value: T) {
        let sender = self.permit.take().expect("Permit invariant violated!");
        sender.send(value);
        if let Some(sent_ref) = self.sent_ref {
            sent_ref.inc();
        }
        // skip the drop logic, see https://github.com/tokio-rs/tokio/blob/a66884a2fb80d1180451706f3c3e006a3fdcb036/tokio/src/sync/mpsc/bounded.rs#L1155-L1163
        std::mem::forget(self);
    }
}

impl<'a, T> Drop for Permit<'a, T> {
    fn drop(&mut self) {
        // In the case the permit is dropped without sending, we still want to decrease the occupancy of the channel.
        // Otherwise, receiver should be responsible for decreasing the inflight gauge.
        if self.permit.is_some() {
            if let Some(inflight_ref) = self.inflight_ref {
                inflight_ref.dec();
            }
        }
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

/// Wraps [`mpsc::WeakSender`] with gauges counting the sent and inflight items.
#[derive(Debug)]
pub struct WeakSender<T> {
    inner: mpsc::WeakSender<T>,
    inflight: Option<IntGauge>,
    sent: Option<IntGauge>,
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

/// Wraps [`mpsc::Receiver`] with gauges counting the inflight and received items.
#[derive(Debug)]
pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
    inflight: Option<IntGauge>,
    received: Option<IntGauge>,
}

impl<T> Receiver<T> {
    /// Receives the next value for this receiver.
    /// Decrements the gauge in case of a successful `recv`.
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await.tap(|opt| {
            if opt.is_some() {
                if let Some(inflight) = &self.inflight {
                    inflight.dec();
                }
                if let Some(received) = &self.received {
                    received.inc();
                }
            }
        })
    }

    /// Attempts to receive the next value for this receiver.
    /// Decrements the gauge in case of a successful `try_recv`.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.inner.try_recv().inspect(|_| {
            if let Some(inflight) = &self.inflight {
                inflight.dec();
            }
            if let Some(received) = &self.received {
                received.inc();
            }
        })
    }

    pub fn blocking_recv(&mut self) -> Option<T> {
        self.inner.blocking_recv().inspect(|_| {
            if let Some(inflight) = &self.inflight {
                inflight.dec();
            }
            if let Some(received) = &self.received {
                received.inc();
            }
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
                if let Some(inflight) = &self.inflight {
                    inflight.dec();
                }
                if let Some(received) = &self.received {
                    received.inc();
                }
                res
            }
            s => s,
        }
    }

    /// Returns a reference to the underlying received gauge.
    #[cfg(test)]
    fn received(&self) -> &IntGauge {
        self.received
            .as_ref()
            .expect("Metrics should have initialized")
    }
}

impl<T> Unpin for Receiver<T> {}

/// Wraps [`mpsc::channel()`] to create a pair of `Sender` and `Receiver`
pub fn channel<T>(name: &str, size: usize) -> (Sender<T>, Receiver<T>) {
    let metrics = get_metrics();
    let (sender, receiver) = mpsc::channel(size);
    (
        Sender {
            inner: sender,
            inflight: metrics.map(|m| m.channel_inflight.with_label_values(&[name])),
            sent: metrics.map(|m| m.channel_sent.with_label_values(&[name])),
        },
        Receiver {
            inner: receiver,
            inflight: metrics.map(|m| m.channel_inflight.with_label_values(&[name])),
            received: metrics.map(|m| m.channel_received.with_label_values(&[name])),
        },
    )
}

/// Wraps [`mpsc::UnboundedSender`] with gauges counting the sent and inflight items.
#[derive(Debug)]
pub struct UnboundedSender<T> {
    inner: mpsc::UnboundedSender<T>,
    inflight: Option<IntGauge>,
    sent: Option<IntGauge>,
}

impl<T> UnboundedSender<T> {
    /// Sends a value, waiting until there is capacity.
    /// Increments the gauge in case of a successful `send`.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.inner.send(value).map(|_| {
            if let Some(inflight) = &self.inflight {
                inflight.inc();
            }
            if let Some(sent) = &self.sent {
                sent.inc();
            }
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

    pub fn downgrade(&self) -> WeakUnboundedSender<T> {
        let sender = self.inner.downgrade();
        WeakUnboundedSender {
            inner: sender,
            inflight: self.inflight.clone(),
            sent: self.sent.clone(),
        }
    }

    /// Returns a reference to the underlying inflight gauge.
    #[cfg(test)]
    fn inflight(&self) -> &IntGauge {
        self.inflight
            .as_ref()
            .expect("Metrics should have initialized")
    }

    /// Returns a reference to the underlying sent gauge.
    #[cfg(test)]
    fn sent(&self) -> &IntGauge {
        self.sent.as_ref().expect("Metrics should have initialized")
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

/// Wraps [`mpsc::WeakUnboundedSender`] with gauges counting the sent and inflight items.
#[derive(Debug)]
pub struct WeakUnboundedSender<T> {
    inner: mpsc::WeakUnboundedSender<T>,
    inflight: Option<IntGauge>,
    sent: Option<IntGauge>,
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

/// Wraps [`mpsc::UnboundedReceiver`] with gauges counting the inflight and received items.
#[derive(Debug)]
pub struct UnboundedReceiver<T> {
    inner: mpsc::UnboundedReceiver<T>,
    inflight: Option<IntGauge>,
    received: Option<IntGauge>,
}

impl<T> UnboundedReceiver<T> {
    /// Receives the next value for this receiver.
    /// Decrements the gauge in case of a successful `recv`.
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await.tap(|opt| {
            if opt.is_some() {
                if let Some(inflight) = &self.inflight {
                    inflight.dec();
                }
                if let Some(received) = &self.received {
                    received.inc();
                }
            }
        })
    }

    /// Attempts to receive the next value for this receiver.
    /// Decrements the gauge in case of a successful `try_recv`.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.inner.try_recv().inspect(|_| {
            if let Some(inflight) = &self.inflight {
                inflight.dec();
            }
            if let Some(received) = &self.received {
                received.inc();
            }
        })
    }

    pub fn blocking_recv(&mut self) -> Option<T> {
        self.inner.blocking_recv().inspect(|_| {
            if let Some(inflight) = &self.inflight {
                inflight.dec();
            }
            if let Some(received) = &self.received {
                received.inc();
            }
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
                if let Some(inflight) = &self.inflight {
                    inflight.dec();
                }
                if let Some(received) = &self.received {
                    received.inc();
                }
                res
            }
            s => s,
        }
    }

    /// Returns a reference to the underlying received gauge.
    #[cfg(test)]
    fn received(&self) -> &IntGauge {
        self.received
            .as_ref()
            .expect("Metrics should have initialized")
    }
}

impl<T> Unpin for UnboundedReceiver<T> {}

/// Wraps [`mpsc::unbounded_channel()`] to create a pair of `UnboundedSender` and `UnboundedReceiver`
pub fn unbounded_channel<T>(name: &str) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let metrics = get_metrics();
    #[allow(clippy::disallowed_methods)]
    let (sender, receiver) = mpsc::unbounded_channel();
    (
        UnboundedSender {
            inner: sender,
            inflight: metrics.map(|m| m.channel_inflight.with_label_values(&[name])),
            sent: metrics.map(|m| m.channel_sent.with_label_values(&[name])),
        },
        UnboundedReceiver {
            inner: receiver,
            inflight: metrics.map(|m| m.channel_inflight.with_label_values(&[name])),
            received: metrics.map(|m| m.channel_received.with_label_values(&[name])),
        },
    )
}

#[cfg(test)]
mod test {
    use std::task::{Context, Poll};

    use futures::{task::noop_waker, FutureExt as _};
    use prometheus::Registry;
    use tokio::sync::mpsc::error::TrySendError;

    use crate::{
        init_metrics,
        monitored_mpsc::{channel, unbounded_channel},
    };

    #[tokio::test]
    async fn test_bounded_send_and_receive() {
        init_metrics(&Registry::new());
        let (tx, mut rx) = channel("test_bounded_send_and_receive", 8);
        let inflight = tx.inflight();
        let sent = tx.sent();
        let received = rx.received().clone();

        assert_eq!(inflight.get(), 0);
        let item = 42;
        tx.send(item).await.unwrap();
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        let received_item = rx.recv().await.unwrap();
        assert_eq!(received_item, item);
        assert_eq!(inflight.get(), 0);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 1);
    }

    #[tokio::test]
    async fn test_try_send() {
        init_metrics(&Registry::new());
        let (tx, mut rx) = channel("test_try_send", 1);
        let inflight = tx.inflight();
        let sent = tx.sent();
        let received = rx.received().clone();

        assert_eq!(inflight.get(), 0);
        assert_eq!(sent.get(), 0);
        assert_eq!(received.get(), 0);

        let item = 42;
        tx.try_send(item).unwrap();
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        let received_item = rx.recv().await.unwrap();
        assert_eq!(received_item, item);
        assert_eq!(inflight.get(), 0);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 1);
    }

    #[tokio::test]
    async fn test_try_send_full() {
        init_metrics(&Registry::new());
        let (tx, mut rx) = channel("test_try_send_full", 2);
        let inflight = tx.inflight();
        let sent = tx.sent();
        let received = rx.received().clone();

        assert_eq!(inflight.get(), 0);

        let item = 42;
        tx.try_send(item).unwrap();
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        tx.try_send(item).unwrap();
        assert_eq!(inflight.get(), 2);
        assert_eq!(sent.get(), 2);
        assert_eq!(received.get(), 0);

        if let Err(e) = tx.try_send(item) {
            assert!(matches!(e, TrySendError::Full(_)));
        } else {
            panic!("Expect try_send return channel being full error");
        }
        assert_eq!(inflight.get(), 2);
        assert_eq!(sent.get(), 2);
        assert_eq!(received.get(), 0);

        let received_item = rx.recv().await.unwrap();
        assert_eq!(received_item, item);
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 2);
        assert_eq!(received.get(), 1);

        let received_item = rx.recv().await.unwrap();
        assert_eq!(received_item, item);
        assert_eq!(inflight.get(), 0);
        assert_eq!(sent.get(), 2);
        assert_eq!(received.get(), 2);
    }

    #[tokio::test]
    async fn test_unbounded_send_and_receive() {
        init_metrics(&Registry::new());
        let (tx, mut rx) = unbounded_channel("test_unbounded_send_and_receive");
        let inflight = tx.inflight();
        let sent = tx.sent();
        let received = rx.received().clone();

        assert_eq!(inflight.get(), 0);
        let item = 42;
        tx.send(item).unwrap();
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        let received_item = rx.recv().await.unwrap();
        assert_eq!(received_item, item);
        assert_eq!(inflight.get(), 0);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 1);
    }

    #[tokio::test]
    async fn test_empty_closed_channel() {
        init_metrics(&Registry::new());
        let (tx, mut rx) = channel("test_empty_closed_channel", 8);
        let inflight = tx.inflight();
        let received = rx.received().clone();

        assert_eq!(inflight.get(), 0);
        let item = 42;
        tx.send(item).await.unwrap();
        assert_eq!(inflight.get(), 1);
        assert_eq!(received.get(), 0);

        let received_item = rx.recv().await.unwrap();
        assert_eq!(received_item, item);
        assert_eq!(inflight.get(), 0);
        assert_eq!(received.get(), 1);

        // channel is empty
        let res = rx.try_recv();
        assert!(res.is_err());
        assert_eq!(inflight.get(), 0);
        assert_eq!(received.get(), 1);

        // channel is closed
        rx.close();
        let res2 = rx.recv().now_or_never().unwrap();
        assert!(res2.is_none());
        assert_eq!(inflight.get(), 0);
        assert_eq!(received.get(), 1);
    }

    #[tokio::test]
    async fn test_reserve() {
        init_metrics(&Registry::new());
        let (tx, mut rx) = channel("test_reserve", 8);
        let inflight = tx.inflight();
        let sent = tx.sent();
        let received = rx.received().clone();

        assert_eq!(inflight.get(), 0);

        let permit = tx.reserve().await.unwrap();
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 0);
        assert_eq!(received.get(), 0);

        let item = 42;
        permit.send(item);
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        let permit_2 = tx.reserve().await.unwrap();
        assert_eq!(inflight.get(), 2);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        drop(permit_2);
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        let received_item = rx.recv().await.unwrap();
        assert_eq!(received_item, item);

        assert_eq!(inflight.get(), 0);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 1);
    }

    #[tokio::test]
    async fn test_reserve_and_drop() {
        init_metrics(&Registry::new());
        let (tx, _rx) = channel::<usize>("test_reserve_and_drop", 8);
        let inflight = tx.inflight();

        assert_eq!(inflight.get(), 0);

        let permit = tx.reserve().await.unwrap();
        assert_eq!(inflight.get(), 1);

        drop(permit);

        assert_eq!(inflight.get(), 0);
    }

    #[tokio::test]
    async fn test_send_backpressure() {
        init_metrics(&Registry::new());
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let (tx, mut rx) = channel("test_send_backpressure", 1);
        let inflight = tx.inflight();
        let sent = tx.sent();
        let received = rx.received().clone();

        assert_eq!(inflight.get(), 0);

        tx.send(1).await.unwrap();
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        // channel is full. send should be blocked.
        let mut task = Box::pin(tx.send(2));
        assert!(matches!(task.poll_unpin(&mut cx), Poll::Pending));
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        let item = rx.recv().await.unwrap();
        assert_eq!(item, 1);
        assert_eq!(inflight.get(), 0);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 1);

        assert!(task.now_or_never().is_some());
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 2);
        assert_eq!(received.get(), 1);
    }

    #[tokio::test]
    async fn test_reserve_backpressure() {
        init_metrics(&Registry::new());
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let (tx, mut rx) = channel("test_reserve_backpressure", 1);
        let inflight = tx.inflight();
        let sent = tx.sent();
        let received = rx.received().clone();

        assert_eq!(inflight.get(), 0);

        let permit = tx.reserve().await.unwrap();
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 0);
        assert_eq!(received.get(), 0);

        let mut task = Box::pin(tx.send(2));
        assert!(matches!(task.poll_unpin(&mut cx), Poll::Pending));
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 0);
        assert_eq!(received.get(), 0);

        permit.send(1);
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        let item = rx.recv().await.unwrap();
        assert_eq!(item, 1);
        assert_eq!(inflight.get(), 0);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 1);

        assert!(task.now_or_never().is_some());
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 2);
        assert_eq!(received.get(), 1);
    }

    #[tokio::test]
    async fn test_send_backpressure_multi_senders() {
        init_metrics(&Registry::new());
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let (tx1, mut rx) = channel("test_send_backpressure_multi_senders", 1);
        let inflight = tx1.inflight();
        let sent = tx1.sent();
        let received = rx.received().clone();

        assert_eq!(inflight.get(), 0);

        tx1.send(1).await.unwrap();
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        let tx2 = tx1.clone();
        let mut task = Box::pin(tx2.send(2));
        assert!(matches!(task.poll_unpin(&mut cx), Poll::Pending));
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 0);

        let item = rx.recv().await.unwrap();
        assert_eq!(item, 1);
        assert_eq!(inflight.get(), 0);
        assert_eq!(sent.get(), 1);
        assert_eq!(received.get(), 1);

        assert!(task.now_or_never().is_some());
        assert_eq!(inflight.get(), 1);
        assert_eq!(sent.get(), 2);
        assert_eq!(received.get(), 1);
    }
}
