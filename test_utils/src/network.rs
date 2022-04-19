// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use futures::SinkExt;
use futures::StreamExt;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::codec::Framed;
use tokio_util::codec::LengthDelimitedCodec;

// Fixture: a test network listener.
pub fn test_listener(address: SocketAddr) -> JoinHandle<Bytes> {
    tokio::spawn(async move {
        let listener = TcpListener::bind(&address).await.unwrap();
        let (socket, _) = listener.accept().await.unwrap();
        let mut transport = Framed::new(socket, LengthDelimitedCodec::new());
        match transport.next().await {
            Some(Ok(received)) => {
                transport.send(Bytes::from("Ack")).await.unwrap();
                received.freeze()
            }
            _ => panic!("Failed to receive network message"),
        }
    })
}
