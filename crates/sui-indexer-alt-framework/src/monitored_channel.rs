// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use prometheus::IntGauge;
use tokio::sync::mpsc;

/// Creates a bounded channel whose in-flight item count is tracked by the provided
/// [`IntGauge`]. The gauge is incremented on every successful `send` and decremented
/// on every successful `recv` / `try_recv` / stream poll.
pub fn channel<T>(size: usize, inflight: IntGauge) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::channel(size);
    (
        Sender {
            inner: tx,
            inflight: inflight.clone(),
        },
        Receiver {
            inner: rx,
            inflight,
        },
    )
}

/// Wraps [`mpsc::Sender`] with inline gauge tracking.
pub struct Sender<T> {
    inner: mpsc::Sender<T>,
    inflight: IntGauge,
}

impl<T> Sender<T> {
    pub async fn send(&self, value: T) -> Result<(), mpsc::error::SendError<T>> {
        self.inner.send(value).await?;
        self.inflight.inc();
        Ok(())
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            inflight: self.inflight.clone(),
        }
    }
}

/// Wraps [`mpsc::Receiver`] with inline gauge tracking.
pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
    inflight: IntGauge,
}

impl<T> Receiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        let item = self.inner.recv().await;
        if item.is_some() {
            self.inflight.dec();
        }
        item
    }

    pub fn try_recv(&mut self) -> Result<T, mpsc::error::TryRecvError> {
        let item = self.inner.try_recv()?;
        self.inflight.dec();
        Ok(item)
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Polls the inner receiver for use in Stream implementations.
    fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.inner.poll_recv(cx).map(|opt| {
            if opt.is_some() {
                self.inflight.dec();
            }
            opt
        })
    }
}

/// Wraps a [`Receiver`] as a [`futures::Stream`], decrementing the gauge on each
/// yielded item. Use this in place of [`tokio_stream::wrappers::ReceiverStream`].
pub struct ReceiverStream<T> {
    inner: Receiver<T>,
}

impl<T> ReceiverStream<T> {
    pub fn new(recv: Receiver<T>) -> Self {
        Self { inner: recv }
    }
}

impl<T> Stream for ReceiverStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.inner.poll_recv(cx)
    }
}

#[cfg(test)]
mod tests {
    use prometheus::Registry;
    use prometheus::register_int_gauge_with_registry;

    use super::*;

    fn test_gauge() -> IntGauge {
        let registry = Registry::new();
        register_int_gauge_with_registry!("test_inflight", "test gauge", registry).unwrap()
    }

    #[tokio::test]
    async fn gauge_increments_on_send() {
        let gauge = test_gauge();
        let (tx, _rx) = channel::<u32>(8, gauge.clone());
        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        assert_eq!(gauge.get(), 2);
    }

    #[tokio::test]
    async fn gauge_decrements_on_recv() {
        let gauge = test_gauge();
        let (tx, mut rx) = channel::<u32>(8, gauge.clone());
        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        assert_eq!(gauge.get(), 2);

        rx.recv().await.unwrap();
        assert_eq!(gauge.get(), 1);

        rx.recv().await.unwrap();
        assert_eq!(gauge.get(), 0);
    }

    #[tokio::test]
    async fn gauge_decrements_on_try_recv() {
        let gauge = test_gauge();
        let (tx, mut rx) = channel::<u32>(8, gauge.clone());
        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        assert_eq!(gauge.get(), 2);

        rx.try_recv().unwrap();
        assert_eq!(gauge.get(), 1);

        rx.try_recv().unwrap();
        assert_eq!(gauge.get(), 0);
    }

    #[tokio::test]
    async fn try_recv_error_does_not_change_gauge() {
        let gauge = test_gauge();
        let (_tx, mut rx) = channel::<u32>(8, gauge.clone());
        assert!(rx.try_recv().is_err());
        assert_eq!(gauge.get(), 0);
    }

    #[tokio::test]
    async fn recv_returns_none_on_closed_channel() {
        let gauge = test_gauge();
        let (tx, mut rx) = channel::<u32>(8, gauge.clone());
        tx.send(1).await.unwrap();
        drop(tx);

        assert_eq!(rx.recv().await, Some(1));
        assert_eq!(gauge.get(), 0);

        assert_eq!(rx.recv().await, None);
        assert_eq!(gauge.get(), 0);
    }

    #[tokio::test]
    async fn stream_decrements_gauge() {
        use futures::StreamExt;

        let gauge = test_gauge();
        let (tx, rx) = channel::<u32>(8, gauge.clone());
        tx.send(10).await.unwrap();
        tx.send(20).await.unwrap();
        tx.send(30).await.unwrap();
        drop(tx);
        assert_eq!(gauge.get(), 3);

        let mut stream = ReceiverStream::new(rx);
        assert_eq!(stream.next().await, Some(10));
        assert_eq!(gauge.get(), 2);

        assert_eq!(stream.next().await, Some(20));
        assert_eq!(gauge.get(), 1);

        assert_eq!(stream.next().await, Some(30));
        assert_eq!(gauge.get(), 0);

        assert_eq!(stream.next().await, None);
        assert_eq!(gauge.get(), 0);
    }

    #[tokio::test]
    async fn cloned_sender_shares_gauge() {
        let gauge = test_gauge();
        let (tx, mut rx) = channel::<u32>(8, gauge.clone());
        let tx2 = tx.clone();

        tx.send(1).await.unwrap();
        tx2.send(2).await.unwrap();
        assert_eq!(gauge.get(), 2);

        rx.recv().await.unwrap();
        assert_eq!(gauge.get(), 1);
    }
}
