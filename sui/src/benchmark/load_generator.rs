// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Error;
use bytes::Bytes;
use futures::{
    channel::mpsc::{channel as MpscChannel, Receiver, Sender as MpscSender},
    stream::StreamExt,
    SinkExt,
};
use rayon::prelude::*;
use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};
use sui_core::{
    authority::*,
    authority_client::{AuthorityAPI, NetworkAuthorityClient},
    authority_server::{AuthorityServer, AuthorityServerHandle},
};
use sui_network::network::{NetworkClient, NetworkServer};
use sui_types::{messages::*, serialize::*};
use tokio::{sync::Notify, time};
use tracing::{error, info};

pub fn check_transaction_response(reply_message: Result<SerializedMessage, Error>) {
    match reply_message {
        Ok(SerializedMessage::TransactionResp(res)) => {
            if let Some(e) = res.signed_effects {
                if matches!(e.effects.status, ExecutionStatus::Failure { .. }) {
                    info!("Execution Error {:?}", e.effects.status);
                }
            }
        }
        Err(err) => {
            error!("Received Error {:?}", err);
        }
        Ok(q) => error!("Received invalid response {:?}", q),
    };
}

pub async fn send_tx_chunks(
    tx_chunks: Vec<Bytes>,
    net_client: NetworkClient,
    _conn: usize,
) -> (u128, Vec<Result<Vec<u8>, io::Error>>) {
    let time_start = Instant::now();

    // This probably isn't going to be as fast so we probably want to provide away to send a batch
    // of txns to the authority at a time
    let client = NetworkAuthorityClient::new(net_client);
    let mut tx_resp = Vec::new();
    for tx in tx_chunks {
        let message = deserialize_message(&tx[..]).unwrap();
        let resp = match message {
            SerializedMessage::Transaction(transaction) => client
                .handle_transaction(*transaction)
                .await
                .map(|resp| serialize_transaction_info(&resp))
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e)),
            SerializedMessage::Cert(cert) => client
                .handle_confirmation_transaction(ConfirmationTransaction { certificate: *cert })
                .await
                .map(|resp| serialize_transaction_info(&resp))
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e)),
            _ => panic!("unexpected message type"),
        };
        tx_resp.push(resp);
    }

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
) -> AuthorityServerHandle {
    // The following two fields are only needed for shared objects (not by this bench).
    let consensus_address = "127.0.0.1:0".parse().unwrap();
    let (tx_consensus_listener, _rx_consensus_listener) = tokio::sync::mpsc::channel(1);

    let server = AuthorityServer::new(
        network_server.base_address,
        network_server.base_port,
        network_server.buffer_size,
        Arc::new(state),
        consensus_address,
        tx_consensus_listener,
    );
    server.spawn().await.unwrap()
}

pub fn calculate_throughput(num_items: usize, elapsed_time_us: u128) -> f64 {
    1_000_000.0 * num_items as f64 / elapsed_time_us as f64
}
