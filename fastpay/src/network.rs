// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::transport::*;
use fastpay_core::{authority::*, client::*};
use fastx_types::{error::*, messages::*, serialize::*};

use async_trait::async_trait;
use bytes::Bytes;
use futures::future::FutureExt;
use log::*;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time;

pub struct Server {
    base_address: String,
    base_port: u32,
    state: AuthorityState,
    buffer_size: usize,
    // Stats
    packets_processed: AtomicUsize,
    user_errors: AtomicUsize,
}

impl Server {
    pub fn new(
        base_address: String,
        base_port: u32,
        state: AuthorityState,
        buffer_size: usize,
    ) -> Self {
        Self {
            base_address,
            base_port,
            state,
            buffer_size,
            packets_processed: AtomicUsize::new(0),
            user_errors: AtomicUsize::new(0),
        }
    }

    pub fn packets_processed(&self) -> usize {
        self.packets_processed.load(Ordering::Relaxed)
    }

    pub fn user_errors(&self) -> usize {
        self.user_errors.load(Ordering::Relaxed)
    }

    pub async fn spawn(self) -> Result<SpawnedServer, io::Error> {
        info!(
            "Listening to TCP traffic on {}:{}",
            self.base_address, self.base_port
        );
        let address = format!("{}:{}", self.base_address, self.base_port);

        let buffer_size = self.buffer_size;

        let state = RunningServerState { server: self };
        // Launch server for the appropriate protocol.
        spawn_server(&address, state, buffer_size).await
    }
}

struct RunningServerState {
    server: Server,
}

impl MessageHandler for RunningServerState {
    fn handle_message<'a>(
        &'a self,
        buffer: &'a [u8],
    ) -> futures::future::BoxFuture<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            let result = deserialize_message(buffer);
            let reply = match result {
                Err(_) => Err(FastPayError::InvalidDecoding),
                Ok(result) => {
                    match result {
                        SerializedMessage::Order(message) => self
                            .server
                            .state
                            .handle_order(*message)
                            .await
                            .map(|info| Some(serialize_order_info(&info))),
                        SerializedMessage::Cert(message) => {
                            let confirmation_order = ConfirmationOrder {
                                certificate: message.as_ref().clone(),
                            };
                            match self
                                .server
                                .state
                                .handle_confirmation_order(confirmation_order)
                                .await
                            {
                                Ok(info) => {
                                    // Response
                                    Ok(Some(serialize_order_info(&info)))
                                }
                                Err(error) => Err(error),
                            }
                        }
                        SerializedMessage::AccountInfoReq(message) => self
                            .server
                            .state
                            .handle_account_info_request(*message)
                            .await
                            .map(|info| Some(serialize_account_info_response(&info))),
                        SerializedMessage::ObjectInfoReq(message) => self
                            .server
                            .state
                            .handle_object_info_request(*message)
                            .await
                            .map(|info| Some(serialize_object_info_response(&info))),
                        _ => Err(FastPayError::UnexpectedMessage),
                    }
                }
            };

            self.server
                .packets_processed
                .fetch_add(1, Ordering::Relaxed);

            if self.server.packets_processed() % 5000 == 0 {
                info!(
                    "{}:{} has processed {} packets",
                    self.server.base_address,
                    self.server.base_port,
                    self.server.packets_processed()
                );
            }

            match reply {
                Ok(x) => x,
                Err(error) => {
                    warn!("User query failed: {}", error);
                    self.server.user_errors.fetch_add(1, Ordering::Relaxed);
                    Some(serialize_error(&error))
                }
            }
        })
    }
}

#[derive(Clone)]
pub struct Client {
    base_address: String,
    base_port: u32,
    buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
}

impl Client {
    pub fn new(
        base_address: String,
        base_port: u32,
        buffer_size: usize,
        send_timeout: std::time::Duration,
        recv_timeout: std::time::Duration,
    ) -> Self {
        Self {
            base_address,
            base_port,
            buffer_size,
            send_timeout,
            recv_timeout,
        }
    }

    async fn send_recv_bytes_internal(&self, buf: Vec<u8>) -> Result<Vec<u8>, io::Error> {
        let address = format!("{}:{}", self.base_address, self.base_port);
        let mut stream = connect(address, self.buffer_size).await?;
        // Send message
        time::timeout(self.send_timeout, stream.write_data(&buf)).await??;
        // Wait for reply
        time::timeout(self.recv_timeout, stream.read_data()).await?
    }

    pub async fn send_recv_bytes<T>(
        &self,
        buf: Vec<u8>,
        deserializer: fn(SerializedMessage) -> Result<T, FastPayError>,
    ) -> Result<T, FastPayError> {
        match self.send_recv_bytes_internal(buf).await {
            Err(error) => Err(FastPayError::ClientIoError {
                error: format!("{}", error),
            }),
            Ok(response) => {
                // Parse reply
                match deserialize_message(&response[..]) {
                    Ok(SerializedMessage::Error(error)) => Err(*error),
                    Ok(message) => deserializer(message),
                    Err(_) => Err(FastPayError::InvalidDecoding),
                    // _ => Err(FastPayError::UnexpectedMessage),
                }
            }
        }
    }
}

#[async_trait]
impl AuthorityClient for Client {
    /// Initiate a new transfer to a FastPay or Primary account.
    async fn handle_order(&mut self, order: Order) -> Result<OrderInfoResponse, FastPayError> {
        self.send_recv_bytes(serialize_order(&order), order_info_deserializer)
            .await
    }

    /// Confirm a transfer to a FastPay or Primary account.
    async fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> Result<OrderInfoResponse, FastPayError> {
        self.send_recv_bytes(serialize_cert(&order.certificate), order_info_deserializer)
            .await
    }

    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, FastPayError> {
        self.send_recv_bytes(
            serialize_account_info_request(&request),
            account_info_deserializer,
        )
        .await
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, FastPayError> {
        self.send_recv_bytes(
            serialize_object_info_request(&request),
            object_info_deserializer,
        )
        .await
    }
}

#[derive(Clone)]
pub struct MassClient {
    base_address: String,
    base_port: u32,
    buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
    max_in_flight: u64,
}

impl MassClient {
    pub fn new(
        base_address: String,
        base_port: u32,
        buffer_size: usize,
        send_timeout: std::time::Duration,
        recv_timeout: std::time::Duration,
        max_in_flight: u64,
    ) -> Self {
        Self {
            base_address,
            base_port,
            buffer_size,
            send_timeout,
            recv_timeout,
            max_in_flight,
        }
    }

    async fn run_core(&self, requests: Vec<Bytes>) -> Result<Vec<Bytes>, io::Error> {
        let address = format!("{}:{}", self.base_address, self.base_port);
        let mut stream = connect(address, self.buffer_size).await?;
        let mut requests = requests.iter();
        let mut in_flight: u64 = 0;
        let mut responses = Vec::new();

        loop {
            while in_flight < self.max_in_flight {
                let request = match requests.next() {
                    None => {
                        if in_flight == 0 {
                            return Ok(responses);
                        }
                        // No more entries to send.
                        break;
                    }
                    Some(request) => request,
                };
                let status = time::timeout(self.send_timeout, stream.write_data(request)).await;
                if let Err(error) = status {
                    error!("Failed to send request: {}", error);
                    continue;
                }
                in_flight += 1;
            }
            if requests.len() % 5000 == 0 && requests.len() > 0 {
                info!("In flight {} Remaining {}", in_flight, requests.len());
            }
            match time::timeout(self.recv_timeout, stream.read_data()).await {
                Ok(Ok(buffer)) => {
                    in_flight -= 1;
                    responses.push(Bytes::from(buffer));
                }
                Ok(Err(error)) => {
                    if error.kind() == io::ErrorKind::UnexpectedEof {
                        info!("Socket closed by server");
                        return Ok(responses);
                    }
                    error!("Received error response: {}", error);
                }
                Err(error) => {
                    error!(
                        "Timeout while receiving response: {} (in flight: {})",
                        error, in_flight
                    );
                }
            }
        }
    }

    /// Spin off one task on this authority client.
    pub fn run<I>(
        &self,
        requests: I,
        connections: usize,
    ) -> impl futures::stream::Stream<Item = Vec<Bytes>>
    where
        I: IntoIterator<Item = Bytes>,
    {
        let handles = futures::stream::FuturesUnordered::new();

        let outer_requests: Vec<_> = requests.into_iter().collect();
        let size = outer_requests.len() / connections;
        for chunk in outer_requests[..].chunks(size) {
            let requests: Vec<_> = chunk.to_vec();
            let client = self.clone();
            handles.push(
                tokio::spawn(async move {
                    info!(
                        "Sending TCP requests to {}:{}",
                        client.base_address, client.base_port,
                    );
                    let responses = client
                        .run_core(requests)
                        .await
                        .unwrap_or_else(|_| Vec::new());
                    info!(
                        "Done sending TCP requests to {}:{}",
                        client.base_address, client.base_port,
                    );
                    responses
                })
                .then(|x| async { x.unwrap_or_else(|_| Vec::new()) }),
            );
        }

        handles
    }
}
