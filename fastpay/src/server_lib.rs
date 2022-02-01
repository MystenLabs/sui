// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use fastpay_core::authority::*;
use fastx_network::transport::*;
use fastx_types::{error::*, messages::*, serialize::*};

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::*;

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
                        SerializedMessage::OrderInfoReq(message) => self
                            .server
                            .state
                            .handle_order_info_request(*message)
                            .await
                            .map(|info| Some(serialize_order_info(&info))),
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
