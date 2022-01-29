// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::transport::*;
use fastx_types::{error::*, serialize::*};

use std::io;
use tokio::time;

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
