// Copyright(C) Facebook, Inc. and its affiliates.
use super::*;
use crate::common::listener;
use futures::future::try_join_all;

#[tokio::test]
async fn send() {
    // Run a TCP server.
    let address = "127.0.0.1:5000".parse::<SocketAddr>().unwrap();
    let message = "Hello, world!";
    let handle = listener(address, message.to_string());

    // Make the network sender and send the message.
    let mut sender = ReliableSender::new();
    let cancel_handler = sender.send(address, Bytes::from(message)).await;

    // Ensure we get back an acknowledgement.
    assert!(cancel_handler.await.is_ok());

    // Ensure the server received the expected message (ie. it did not panic).
    assert!(handle.await.is_ok());
}

#[tokio::test]
async fn broadcast() {
    // Run 3 TCP servers.
    let message = "Hello, world!";
    let (handles, addresses): (Vec<_>, Vec<_>) = (0..3)
        .map(|x| {
            let address = format!("127.0.0.1:{}", 5_200 + x)
                .parse::<SocketAddr>()
                .unwrap();
            (listener(address, message.to_string()), address)
        })
        .collect::<Vec<_>>()
        .into_iter()
        .unzip();

    // Make the network sender and send the message.
    let mut sender = ReliableSender::new();
    let cancel_handlers = sender.broadcast(addresses, Bytes::from(message)).await;

    // Ensure we get back an acknowledgement for each message.
    assert!(try_join_all(cancel_handlers).await.is_ok());

    // Ensure all servers received the broadcast.
    assert!(try_join_all(handles).await.is_ok());
}

#[tokio::test]
async fn retry() {
    // Make the network sender and send the message  (no listeners are running).
    let address = "127.0.0.1:5300".parse::<SocketAddr>().unwrap();
    let message = "Hello, world!";
    let mut sender = ReliableSender::new();
    let cancel_handler = sender.send(address, Bytes::from(message)).await;

    // Run a TCP server.
    sleep(Duration::from_millis(50)).await;
    let handle = listener(address, message.to_string());

    // Ensure we get back an acknowledgement.
    assert!(cancel_handler.await.is_ok());

    // Ensure the server received the message (ie. it did not panic).
    assert!(handle.await.is_ok());
}
