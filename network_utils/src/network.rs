// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::transport::*;
use bytes::{Bytes, BytesMut};
use std::{
    net::TcpListener,
    sync::atomic::{AtomicUsize, Ordering},
};
use sui_types::{error::*, serialize::*};
use tracing::debug;

use std::io;

use futures::stream;
use futures::SinkExt;
use futures::StreamExt;
use tokio::task::JoinError;
use tokio::time;

#[derive(Clone, Debug)]
pub struct NetworkClient {
    base_address: String,
    base_port: u16,
    buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
}

impl NetworkClient {
    pub fn new(
        base_address: String,
        base_port: u16,
        buffer_size: usize,
        send_timeout: std::time::Duration,
        recv_timeout: std::time::Duration,
    ) -> Self {
        NetworkClient {
            base_address,
            base_port,
            buffer_size,
            send_timeout,
            recv_timeout,
        }
    }

    pub async fn connect_for_stream(&self, buf: Vec<u8>) -> Result<TcpDataStream, io::Error> {
        let address = format!("{}:{}", self.base_address, self.base_port);
        let mut tcp_stream = connect(address, self.buffer_size).await?;
        // Send message
        time::timeout(self.send_timeout, tcp_stream.write_data(&buf)).await??;
        Ok(tcp_stream)
    }

    async fn send_recv_bytes_internal(&self, buf: Vec<u8>) -> Result<Option<Vec<u8>>, io::Error> {
        let address = format!("{}:{}", self.base_address, self.base_port);
        let mut stream = connect(address, self.buffer_size).await?;
        // Send message
        time::timeout(self.send_timeout, stream.write_data(&buf)).await??;
        // Wait for reply
        time::timeout(self.recv_timeout, async {
            stream.read_data().await.transpose()
        })
        .await?
    }

    pub async fn send_recv_bytes(&self, buf: Vec<u8>) -> Result<SerializedMessage, SuiError> {
        match self.send_recv_bytes_internal(buf).await {
            Err(error) => Err(SuiError::ClientIoError {
                error: format!("{error}"),
            }),
            Ok(Some(response)) => {
                // Parse reply
                match deserialize_message(&response[..]) {
                    Ok((_, SerializedMessage::Error(error))) => Err(*error),
                    Ok((_, message)) => Ok(message),
                    Err(_) => Err(SuiError::InvalidDecoding),
                    // _ => Err(SuiError::UnexpectedMessage),
                }
            }
            Ok(None) => Err(SuiError::ClientIoError {
                error: "Empty response from authority.".to_string(),
            }),
        }
    }

    async fn batch_send_one_chunk(
        &self,
        requests: Vec<Bytes>,
        _max_in_flight: u64,
    ) -> Vec<Result<BytesMut, std::io::Error>> {
        let address = format!("{}:{}", self.base_address, self.base_port);
        let stream = connect(address, self.buffer_size)
            .await
            .expect("Must be able to connect.");
        let total = requests.len();

        let (read_stream, mut write_stream) = (stream.framed_read, stream.framed_write);

        let mut requests = stream::iter(requests.into_iter().map(Ok));
        tokio::spawn(async move { write_stream.send_all(&mut requests).await });

        let mut received = 0;
        let responses: Vec<Result<BytesMut, std::io::Error>> = read_stream
            .take_while(|_buf| {
                received += 1;
                if received % 5000 == 0 && received > 0 {
                    debug!("Received {received}");
                }
                let xcontinue = received <= total;
                futures::future::ready(xcontinue)
            })
            .collect()
            .await;

        responses
    }

    pub fn batch_send(
        &self,
        requests: Vec<Bytes>,
        connections: usize,
        max_in_flight: u64,
    ) -> impl futures::stream::Stream<Item = Result<Vec<Result<BytesMut, std::io::Error>>, JoinError>>
    {
        let handles = futures::stream::FuturesUnordered::new();

        let outer_requests: Vec<_> = requests.into_iter().collect();
        let size = outer_requests.len() / connections;
        for chunk in outer_requests[..].chunks(size) {
            let requests: Vec<_> = chunk.to_vec();
            let client = self.clone();
            handles.push(
                tokio::spawn(async move {
                    debug!(
                        "Sending TCP requests to {}:{}",
                        client.base_address, client.base_port,
                    );
                    let responses = client.batch_send_one_chunk(requests, max_in_flight).await;
                    // .unwrap_or_else(|_| Vec::new());
                    debug!(
                        "Done sending TCP requests to {}:{}",
                        client.base_address, client.base_port,
                    );
                    responses
                }), // .then(|x| async { x.unwrap_or_else(|_| Vec::new()) }),
            );
        }

        handles
    }
}

pub struct NetworkServer {
    pub base_address: String,
    pub base_port: u16,
    pub buffer_size: usize,
    // Stats
    packets_processed: AtomicUsize,
    user_errors: AtomicUsize,
}

impl NetworkServer {
    pub fn new(base_address: String, base_port: u16, buffer_size: usize) -> Self {
        Self {
            base_address,
            base_port,
            buffer_size,
            packets_processed: AtomicUsize::new(0),
            user_errors: AtomicUsize::new(0),
        }
    }

    pub fn packets_processed(&self) -> usize {
        self.packets_processed.load(Ordering::Relaxed)
    }

    pub fn increment_packets_processed(&self) {
        self.packets_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn user_errors(&self) -> usize {
        self.user_errors.load(Ordering::Relaxed)
    }

    pub fn increment_user_errors(&self) {
        self.user_errors.fetch_add(1, Ordering::Relaxed);
    }
}

pub struct PortAllocator {
    next_port: u16,
}

impl PortAllocator {
    pub fn new(starting_port: u16) -> Self {
        Self {
            next_port: starting_port,
        }
    }
    pub fn next_port(&mut self) -> Option<u16> {
        for port in self.next_port..65535 {
            if TcpListener::bind(("127.0.0.1", port)).is_ok() {
                self.next_port = port + 1;
                return Some(port);
            }
        }
        None
    }
}

pub fn parse_recv_bytes(
    response: Result<Option<Vec<u8>>, io::Error>,
) -> Result<SerializedMessage, SuiError> {
    match response {
        Err(error) => Err(SuiError::ClientIoError {
            error: format!("{error}"),
        }),
        Ok(Some(response)) => {
            // Parse reply
            match deserialize_message(&response[..]) {
                Ok((_, SerializedMessage::Error(error))) => Err(*error),
                Ok((_, message)) => Ok(message),
                Err(_) => Err(SuiError::InvalidDecoding),
            }
        }
        Ok(None) => Err(SuiError::ClientIoError {
            error: "Empty response from authority.".to_string(),
        }),
    }
}
