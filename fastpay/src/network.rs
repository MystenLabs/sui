// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::transport::*;
use fastpay_core::{authority::*, client::*};
use fastx_types::{error::*, messages::*, serialize::*};

use bytes::Bytes;
use futures::future::FutureExt;
use log::*;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time;

pub struct Server {
    network_protocol: NetworkProtocol,
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
        network_protocol: NetworkProtocol,
        base_address: String,
        base_port: u32,
        state: AuthorityState,
        buffer_size: usize,
    ) -> Self {
        Self {
            network_protocol,
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
            "Listening to {} traffic on {}:{}",
            self.network_protocol, self.base_address, self.base_port
        );
        let address = format!("{}:{}", self.base_address, self.base_port);

        let buffer_size = self.buffer_size;
        let protocol = self.network_protocol;
        let state = RunningServerState { server: self };
        // Launch server for the appropriate protocol.
        protocol.spawn_server(&address, state, buffer_size).await
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
                            .map(|info| Some(serialize_info_response(&info.into()))),
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
                                    Ok(Some(serialize_info_response(&info.into())))
                                }
                                Err(error) => Err(error),
                            }
                        }
                        SerializedMessage::InfoReq(message) => self
                            .server
                            .state
                            .handle_info_request(*message)
                            .await
                            .map(|info| Some(serialize_info_response(&info))),
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
    network_protocol: NetworkProtocol,
    base_address: String,
    base_port: u32,
    buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
}

impl Client {
    pub fn new(
        network_protocol: NetworkProtocol,
        base_address: String,
        base_port: u32,
        buffer_size: usize,
        send_timeout: std::time::Duration,
        recv_timeout: std::time::Duration,
    ) -> Self {
        Self {
            network_protocol,
            base_address,
            base_port,
            buffer_size,
            send_timeout,
            recv_timeout,
        }
    }

    async fn send_recv_bytes_internal(&self, buf: Vec<u8>) -> Result<Vec<u8>, io::Error> {
        let address = format!("{}:{}", self.base_address, self.base_port);
        let mut stream = self
            .network_protocol
            .connect(address, self.buffer_size)
            .await?;
        // Send message
        time::timeout(self.send_timeout, stream.write_data(&buf)).await??;
        // Wait for reply
        time::timeout(self.recv_timeout, stream.read_data()).await?
    }

    pub async fn send_recv_bytes(&self, buf: Vec<u8>) -> Result<InfoResponse, FastPayError> {
        match self.send_recv_bytes_internal(buf).await {
            Err(error) => Err(FastPayError::ClientIoError {
                error: format!("{}", error),
            }),
            Ok(response) => {
                // Parse reply
                match deserialize_message(&response[..]) {
                    Ok(SerializedMessage::InfoResp(resp)) => Ok(*resp),
                    Ok(SerializedMessage::Error(error)) => Err(*error),
                    Err(_) => Err(FastPayError::InvalidDecoding),
                    _ => Err(FastPayError::UnexpectedMessage),
                }
            }
        }
    }
}

impl AuthorityClient for Client {
    /// Initiate a new transfer to a FastPay or Primary account.
    fn handle_order(&mut self, order: Order) -> AsyncResult<'_, ObjectInfoResponse, FastPayError> {
        Box::pin(async move {
            self.send_recv_bytes(serialize_order(&order))
                .await
                .map(|response| response.into())
        })
    }

    /// Confirm a transfer to a FastPay or Primary account.
    fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> AsyncResult<'_, ObjectInfoResponse, FastPayError> {
        Box::pin(async move {
            self.send_recv_bytes(serialize_cert(&order.certificate))
                .await
                .map(|response| response.into())
        })
    }

    fn handle_info_request(
        &self,
        request: InfoRequest,
    ) -> AsyncResult<'_, InfoResponse, FastPayError> {
        Box::pin(async move { self.send_recv_bytes(serialize_info_request(&request)).await })
    }
}

#[derive(Clone)]
pub struct MassClient {
    network_protocol: NetworkProtocol,
    base_address: String,
    base_port: u32,
    buffer_size: usize,
    send_timeout: std::time::Duration,
    recv_timeout: std::time::Duration,
    max_in_flight: u64,
}

impl MassClient {
    pub fn new(
        network_protocol: NetworkProtocol,
        base_address: String,
        base_port: u32,
        buffer_size: usize,
        send_timeout: std::time::Duration,
        recv_timeout: std::time::Duration,
        max_in_flight: u64,
    ) -> Self {
        Self {
            network_protocol,
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
        let mut stream = self
            .network_protocol
            .connect(address, self.buffer_size)
            .await?;
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
                        "Sending {} requests to {}:{}",
                        client.network_protocol, client.base_address, client.base_port,
                    );
                    let responses = client
                        .run_core(requests)
                        .await
                        .unwrap_or_else(|_| Vec::new());
                    info!(
                        "Done sending {} requests to {}:{}",
                        client.network_protocol, client.base_address, client.base_port,
                    );
                    responses
                })
                .then(|x| async { x.unwrap_or_else(|_| Vec::new()) }),
            );
        }

        handles
    }
}
