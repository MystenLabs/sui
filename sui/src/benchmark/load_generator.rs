// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use anyhow::Error;
use bytes::{Bytes, BytesMut};
use futures::channel::mpsc::{channel as MpscChannel, Receiver, Sender as MpscSender};
use futures::stream::StreamExt;
use futures::SinkExt;

use rayon::prelude::*;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_core::authority::*;
use sui_core::authority_server::AuthorityServer;
use sui_network::network::{NetworkClient, NetworkServer};
use sui_network::transport;
use sui_types::{message_headers::*, messages::*, serialize::*};
use tokio::sync::Notify;
use tokio::time;
use tracing::{error, info};

pub fn check_transaction_response(
    reply_message: Result<(Option<Headers>, SerializedMessage), Error>,
) {
    match reply_message {
        Ok((_, SerializedMessage::TransactionResp(res))) => {
            if let Some(e) = res.signed_effects {
                if matches!(e.effects.status, ExecutionStatus::Failure { .. }) {
                    info!("Execution Error {:?}", e.effects.status);
                }
            }
        }
        Ok((_, q)) => error!("Received invalid response {:?}", q),
        Err(err) => {
            error!("Received Error {:?}", err);
        }
    };
}

pub async fn send_tx_chunks(
    tx_chunks: Vec<Bytes>,
    net_client: NetworkClient,
    conn: usize,
) -> (u128, Vec<Result<BytesMut, io::Error>>) {
    let time_start = Instant::now();

    let tx_resp = net_client
        .batch_send(tx_chunks, conn, 0)
        .map(|x| x.unwrap())
        .concat()
        .await;

    let elapsed = time_start.elapsed().as_micros();

    (elapsed, tx_resp)
}

async fn send_tx_chunks_notif(
    notif: Arc<Notify>,
    tx_chunk: Vec<Bytes>,
    result_chann_tx: &mut MpscSender<u128>,
    net_client: NetworkClient,
    conn: usize,
) {
    notif.notified().await;
    let r = send_tx_chunks(tx_chunk, net_client, conn).await;
    result_chann_tx.send(r.0).await.unwrap();

    let _: Vec<_> =
        r.1.par_iter()
            .map(|q| check_transaction_response(deserialize_message(&(q.as_ref().unwrap())[..])))
            .collect();
}

pub struct FixedRateLoadGenerator {
    /// The time between sending transactions chunks
    /// Anything below 10ms causes degradation in resolution
    pub period_us: u64,
    /// The network client to send transactions on
    pub network_client: NetworkClient,

    pub tick_notifier: Arc<Notify>,

    /// Number of TCP connections to open
    pub connections: usize,

    pub transactions: Vec<Bytes>,

    pub results_chann_rx: Receiver<u128>,

    /// This is the chunk size actually assigned for each tick per task
    /// It is 2*chunk_size due to order and confirmation steps
    pub chunk_size_per_task: usize,
}

// new -> ready -> start

impl FixedRateLoadGenerator {
    pub async fn new(
        transactions: Vec<Bytes>,
        period_us: u64,
        network_client: NetworkClient,
        connections: usize,
    ) -> Self {
        let mut handles = vec![];
        let tick_notifier = Arc::new(Notify::new());

        let (result_chann_tx, results_chann_rx) = MpscChannel(transactions.len() * 2);

        let conn = connections;
        // Spin up a bunch of worker tasks
        // Give each task
        // Step by 2*conn due to order+confirmation, with `conn` tcp connections
        // Take up to 2*conn for each task
        let num_chunks_per_task = conn * 2;
        for tx_chunk in transactions[..].chunks(num_chunks_per_task) {
            let notif = tick_notifier.clone();
            let mut result_chann_tx = result_chann_tx.clone();
            let tx_chunk = tx_chunk.to_vec();
            let client = network_client.clone();

            handles.push(tokio::spawn(async move {
                send_tx_chunks_notif(notif, tx_chunk, &mut result_chann_tx, client, conn).await;
            }));
        }

        drop(result_chann_tx);

        Self {
            period_us,
            network_client,
            transactions,
            connections,
            results_chann_rx,
            tick_notifier,
            chunk_size_per_task: num_chunks_per_task,
        }
    }

    pub async fn start(&mut self) -> Vec<u128> {
        let mut interval = time::interval(Duration::from_micros(self.period_us));
        let mut count = 0;
        loop {
            tokio::select! {
                _  = interval.tick() => {
                    self.tick_notifier.notify_one();
                    count += self.chunk_size_per_task;
                    if count >= self.transactions.len() {
                        break;
                    }
                }
            }
        }
        let mut times = Vec::new();
        while let Some(v) = time::timeout(Duration::from_secs(10), self.results_chann_rx.next())
            .await
            .unwrap_or(None)
        {
            times.push(v);
        }

        times
    }
}

pub async fn spawn_authority_server(
    network_server: NetworkServer,
    state: AuthorityState,
) -> transport::SpawnedServer<AuthorityServer> {
    let server = AuthorityServer::new(
        network_server.base_address,
        network_server.base_port,
        network_server.buffer_size,
        state,
    );
    server.spawn().await.unwrap()
}

pub fn calculate_throughput(num_items: usize, elapsed_time_us: u128) -> f64 {
    1_000_000.0 * num_items as f64 / elapsed_time_us as f64
}
