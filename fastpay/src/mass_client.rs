// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use fastx_network::transport::*;
use futures::future::FutureExt;
use std::io;
use tokio::time;
use tracing::*;

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
