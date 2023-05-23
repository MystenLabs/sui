// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::{channel, channel_with_total};
use futures::{
    task::{noop_waker, Context, Poll},
    FutureExt,
};
use prometheus::{IntCounter, IntGauge};
use tokio::sync::mpsc::error::TrySendError;

#[tokio::test]
async fn test_send() {
    let counter = IntGauge::new("TEST_COUNTER", "test").unwrap();
    let (tx, mut rx) = channel(8, &counter);

    assert_eq!(counter.get(), 0);
    let item = 42;
    tx.send(item).await.unwrap();
    assert_eq!(counter.get(), 1);
    let received_item = rx.recv().await.unwrap();
    assert_eq!(received_item, item);
    assert_eq!(counter.get(), 0);
}

#[tokio::test]
async fn test_total() {
    let counter = IntGauge::new("TEST_COUNTER", "test").unwrap();
    let counter_total = IntCounter::new("TEST_TOTAL", "test_total").unwrap();
    let (tx, mut rx) = channel_with_total(8, &counter, &counter_total);

    assert_eq!(counter.get(), 0);
    let item = 42;
    tx.send(item).await.unwrap();
    assert_eq!(counter.get(), 1);
    let received_item = rx.recv().await.unwrap();
    assert_eq!(received_item, item);
    assert_eq!(counter.get(), 0);
    assert_eq!(counter_total.get(), 1);
}

#[tokio::test]
async fn test_empty_closed_channel() {
    let counter = IntGauge::new("TEST_COUNTER", "test").unwrap();
    let (tx, mut rx) = channel(8, &counter);

    assert_eq!(counter.get(), 0);
    let item = 42;
    tx.send(item).await.unwrap();
    assert_eq!(counter.get(), 1);

    let received_item = rx.recv().await.unwrap();
    assert_eq!(received_item, item);
    assert_eq!(counter.get(), 0);

    // channel is empty
    let res = rx.try_recv();
    assert!(res.is_err());
    assert_eq!(counter.get(), 0);

    // channel is closed
    rx.close();
    let res2 = rx.recv().now_or_never().unwrap();
    assert!(res2.is_none());
    assert_eq!(counter.get(), 0);
}

#[tokio::test]
async fn test_reserve() {
    let counter = IntGauge::new("TEST_COUNTER", "test").unwrap();
    let (tx, mut rx) = channel(8, &counter);

    assert_eq!(counter.get(), 0);
    let item = 42;
    let permit = tx.reserve().await.unwrap();
    assert_eq!(counter.get(), 1);

    permit.send(item);
    let received_item = rx.recv().await.unwrap();

    assert_eq!(received_item, item);
    assert_eq!(counter.get(), 0);
}

#[tokio::test]
async fn test_reserve_and_drop() {
    let counter = IntGauge::new("TEST_COUNTER", "test").unwrap();
    let (tx, _rx) = channel::<i32>(8, &counter);

    assert_eq!(counter.get(), 0);

    let permit = tx.reserve().await.unwrap();
    assert_eq!(counter.get(), 1);

    drop(permit);

    assert_eq!(counter.get(), 0);
}

#[tokio::test]
async fn test_send_backpressure() {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    let counter = IntGauge::new("TEST_COUNTER", "test").unwrap();
    let (tx, mut rx) = channel(1, &counter);

    assert_eq!(counter.get(), 0);
    tx.send(1).await.unwrap();
    assert_eq!(counter.get(), 1);

    let mut task = Box::pin(tx.send(2));
    assert!(matches!(task.poll_unpin(&mut cx), Poll::Pending));
    let item = rx.recv().await.unwrap();
    assert_eq!(item, 1);
    assert_eq!(counter.get(), 0);
    assert!(task.now_or_never().is_some());
    assert_eq!(counter.get(), 1);
}

#[tokio::test]
async fn test_reserve_backpressure() {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);

    let counter = IntGauge::new("TEST_COUNTER", "test").unwrap();
    let (tx, mut rx) = channel(1, &counter);

    assert_eq!(counter.get(), 0);
    let permit = tx.reserve().await.unwrap();
    assert_eq!(counter.get(), 1);

    let mut task = Box::pin(tx.send(2));
    assert!(matches!(task.poll_unpin(&mut cx), Poll::Pending));

    permit.send(1);
    let item = rx.recv().await.unwrap();
    assert_eq!(item, 1);
    assert_eq!(counter.get(), 0);
    assert!(task.now_or_never().is_some());
    assert_eq!(counter.get(), 1);
}

#[tokio::test]
async fn test_send_backpressure_multi_senders() {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let counter = IntGauge::new("TEST_COUNTER", "test").unwrap();
    let (tx1, mut rx) = channel(1, &counter);

    assert_eq!(counter.get(), 0);
    tx1.send(1).await.unwrap();
    assert_eq!(counter.get(), 1);

    let tx2 = tx1;
    let mut task = Box::pin(tx2.send(2));
    assert!(matches!(task.poll_unpin(&mut cx), Poll::Pending));
    let item = rx.recv().await.unwrap();
    assert_eq!(item, 1);
    assert_eq!(counter.get(), 0);
    assert!(task.now_or_never().is_some());
    assert_eq!(counter.get(), 1);
}

#[tokio::test]
async fn test_try_send() {
    let counter = IntGauge::new("TEST_COUNTER", "test").unwrap();
    let (tx, mut rx) = channel(1, &counter);

    assert_eq!(counter.get(), 0);
    let item = 42;
    tx.try_send(item).unwrap();
    assert_eq!(counter.get(), 1);
    let received_item = rx.recv().await.unwrap();
    assert_eq!(received_item, item);
    assert_eq!(counter.get(), 0);
}

#[tokio::test]
async fn test_try_send_full() {
    let counter = IntGauge::new("TEST_COUNTER", "test").unwrap();
    let (tx, mut rx) = channel(2, &counter);

    assert_eq!(counter.get(), 0);
    let item = 42;
    tx.try_send(item).unwrap();
    assert_eq!(counter.get(), 1);
    tx.try_send(item).unwrap();
    assert_eq!(counter.get(), 2);
    if let Err(e) = tx.try_send(item) {
        assert!(matches!(e, TrySendError::Full(_)));
    } else {
        panic!("Expect try_send return channel being full error");
    }

    let received_item = rx.recv().await.unwrap();
    assert_eq!(received_item, item);
    assert_eq!(counter.get(), 1);
    let received_item = rx.recv().await.unwrap();
    assert_eq!(received_item, item);
    assert_eq!(counter.get(), 0);
}
