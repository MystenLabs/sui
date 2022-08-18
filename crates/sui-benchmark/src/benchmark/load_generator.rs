// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::{
    channel::mpsc::{channel as MpscChannel, Receiver, Sender as MpscSender},
    future::try_join_all,
    stream::StreamExt,
    SinkExt,
};
use multiaddr::Multiaddr;
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
use sui_types::messages::*;
use tokio::{sync::Notify, time};
use tracing::{error, info};

use sui_config::NetworkConfig;
use sui_types::committee::StakeUnit;

pub fn check_transaction_response(reply_message: Result<TransactionInfoResponse, io::Error>) {
    match reply_message {
        Ok(res) => {
            if let Some(e) = res.signed_effects {
                if matches!(e.effects().status, ExecutionStatus::Failure { .. }) {
                    info!("Execution Error {:?}", e.effects().status);
                }
            }
        }
        Err(err) => {
            error!("Received Error {:?}", err);
        }
    };
}

pub async fn send_tx_chunks(
    tx_chunks: Vec<(Transaction, CertifiedTransaction)>,
    address: Multiaddr,
    conn: usize,
) -> (u128, Vec<Result<TransactionInfoResponse, io::Error>>) {
    let time_start = Instant::now();

    let mut tasks = Vec::new();
    for tx_chunks in tx_chunks.chunks(tx_chunks.len() / conn) {
        let client = NetworkAuthorityClient::connect_lazy(&address).unwrap();
        let txns = tx_chunks.to_vec();

        let task = tokio::spawn(async move {
            let mut resps = Vec::new();
            for (transaction, certificate) in txns {
                let resp1 = client
                    .handle_transaction(transaction)
                    .await
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e));
                let resp2 = client
                    .handle_certificate(certificate)
                    .await
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e));
                resps.push(resp1);
                resps.push(resp2);
            }
            resps
        });
        tasks.push(task);
    }

    let tx_resp = try_join_all(tasks)
        .await
        .unwrap()
        .into_iter()
        .flatten()
        .collect();

    let elapsed = time_start.elapsed().as_micros();

    (elapsed, tx_resp)
}

pub async fn send_transactions(
    tx_chunks: Vec<Transaction>,
    address: Multiaddr,
    conn: usize,
) -> (u128, Vec<Result<TransactionInfoResponse, io::Error>>) {
    let time_start = Instant::now();

    let mut tasks = Vec::new();
    for tx_chunks in tx_chunks.chunks(tx_chunks.len() / conn) {
        let client = NetworkAuthorityClient::connect_lazy(&address).unwrap();
        let txns = tx_chunks.to_vec();

        let task = tokio::spawn(async move {
            let mut resps = Vec::new();
            for transaction in txns {
                let resp = client
                    .handle_transaction(transaction)
                    .await
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e));
                resps.push(resp);
            }
            resps
        });
        tasks.push(task);
    }

    let tx_resp = try_join_all(tasks)
        .await
        .unwrap()
        .into_iter()
        .flatten()
        .collect();

    let elapsed = time_start.elapsed().as_micros();

    (elapsed, tx_resp)
}

pub async fn send_confs(
    tx_chunks: Vec<CertifiedTransaction>,
    address: Multiaddr,
    conn: usize,
) -> (u128, Vec<Result<TransactionInfoResponse, io::Error>>) {
    let time_start = Instant::now();

    let mut tasks = Vec::new();
    for tx_chunks in tx_chunks.chunks(tx_chunks.len() / conn) {
        let client = NetworkAuthorityClient::connect_lazy(&address).unwrap();
        let txns = tx_chunks.to_vec();

        let task = tokio::spawn(async move {
            let mut resps = Vec::new();
            for certificate in txns {
                let resp = client
                    .handle_certificate(certificate)
                    .await
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e));
                resps.push(resp);
            }
            resps
        });
        tasks.push(task);
    }

    let tx_resp = try_join_all(tasks)
        .await
        .unwrap()
        .into_iter()
        .flatten()
        .collect();

    let elapsed = time_start.elapsed().as_micros();

    (elapsed, tx_resp)
}

async fn send_tx_chunks_notif(
    notif: Arc<Notify>,
    tx_chunk: Vec<(Transaction, CertifiedTransaction)>,
    result_chann_tx: &mut MpscSender<u128>,
    address: Multiaddr,
    conn: usize,
) {
    notif.notified().await;
    let r = send_tx_chunks(tx_chunk, address, conn).await;
    result_chann_tx.send(r.0).await.unwrap();

    let _: Vec<_> =
        r.1.into_par_iter()
            .map(check_transaction_response)
            .collect();
}

pub struct FixedRateLoadGenerator {
    /// The time between sending transactions chunks
    /// Anything below 10ms causes degradation in resolution
    pub period_us: u64,
    /// The address of the validator to send txns to
    pub address: Multiaddr,

    pub tick_notifier: Arc<Notify>,

    /// Number of TCP connections to open
    pub connections: usize,

    pub transactions: Vec<(Transaction, CertifiedTransaction)>,

    pub results_chann_rx: Receiver<u128>,

    /// This is the chunk size actually assigned for each tick per task
    /// It is 2*chunk_size due to order and confirmation steps
    pub chunk_size_per_task: usize,
}

impl FixedRateLoadGenerator {
    pub async fn new(
        transactions: Vec<(Transaction, CertifiedTransaction)>,
        period_us: u64,
        address: Multiaddr,
        connections: usize,
    ) -> Self {
        let mut handles = vec![];
        let tick_notifier = Arc::new(Notify::new());

        let (result_chann_tx, results_chann_rx) = MpscChannel(transactions.len() * 2);

        let conn = connections;
        info!("connections: {connections}");
        // Spin up a bunch of worker tasks
        // Give each task
        // Step by 2*conn due to order+confirmation, with `conn` tcp connections
        // Take up to 2*conn for each task
        let num_chunks_per_task = conn * 2;
        for tx_chunk in transactions[..].chunks(num_chunks_per_task) {
            let notif = tick_notifier.clone();
            let mut result_chann_tx = result_chann_tx.clone();
            let tx_chunk = tx_chunk.to_vec();
            let address = address.clone();

            handles.push(tokio::spawn(async move {
                send_tx_chunks_notif(notif, tx_chunk, &mut result_chann_tx, address, conn).await;
            }));
        }

        drop(result_chann_tx);

        Self {
            period_us,
            address,
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
    listen_address: Multiaddr,
    state: AuthorityState,
) -> AuthorityServerHandle {
    // The following two fields are only needed for shared objects (not by this bench).
    let consensus_address = "/dns/localhost/tcp/0/http".parse().unwrap();
    let (tx_consensus_listener, _rx_consensus_listener) = tokio::sync::mpsc::channel(1);

    let server = AuthorityServer::new(
        listen_address,
        Arc::new(state),
        consensus_address,
        tx_consensus_listener,
    );
    server.spawn().await.unwrap()
}

pub fn calculate_throughput(num_items: usize, elapsed_time_us: u128) -> f64 {
    1_000_000.0 * num_items as f64 / elapsed_time_us as f64
}

async fn send_tx_chunks_for_quorum_notif(
    notif: Arc<Notify>,
    tx_chunk: Vec<Transaction>,
    result_chann_tx: &mut MpscSender<(u128, StakeUnit)>,
    address: Multiaddr,
    stake: StakeUnit,
    conn: usize,
) {
    notif.notified().await;
    let r = send_transactions(tx_chunk, address.clone(), conn).await;

    match result_chann_tx.send((r.0, stake)).await {
        Ok(_) => (),
        Err(e) => {
            // Disconnect is okay since we may leave f running
            if !e.is_disconnected() {
                panic!("Send failed! {:?}", address)
            }
        }
    }

    let _: Vec<_> =
        r.1.into_par_iter()
            .map(check_transaction_response)
            .collect();
}

async fn send_tx_for_quorum(
    notif: Arc<Notify>,
    order_chunk: Vec<Transaction>,
    conf_chunk: Vec<CertifiedTransaction>,
    result_chann_tx: &mut MpscSender<u128>,
    net_clients: Vec<(Multiaddr, StakeUnit)>,
    conn: usize,
    quorum_threshold: StakeUnit,
) {
    // For receiving info back from the subtasks
    let (order_chann_tx, mut order_chann_rx) = MpscChannel(net_clients.len() * 2);

    // Send intent orders to 3f+1
    let order_start_notifier = Arc::new(Notify::new());
    for (net_client, stake) in net_clients.clone() {
        // This is for sending a start signal to the subtasks
        let notif = order_start_notifier.clone();
        // This is for getting the elapsed time
        let mut ch_tx = order_chann_tx.clone();
        // Chunk to send for order_
        let chunk = order_chunk.clone();

        tokio::spawn(async move {
            send_tx_chunks_for_quorum_notif(
                notif,
                chunk,
                &mut ch_tx,
                net_client.clone(),
                stake,
                conn,
            )
            .await;
        });
    }
    drop(order_chann_tx);

    // Wait for timer tick
    notif.notified().await;
    // Notify all the subtasks
    order_start_notifier.notify_waiters();
    // Start timer
    let time_start = Instant::now();

    // Wait for 2f+1 by stake
    let mut total = 0;

    while let Some(v) = time::timeout(Duration::from_secs(10), order_chann_rx.next())
        .await
        .unwrap()
    {
        total += v.1;
        if total >= quorum_threshold {
            break;
        }
    }
    if total < quorum_threshold {
        panic!("Quorum threshold not reached for orders")
    }

    // Confirmation step
    let (conf_chann_tx, mut conf_chann_rx) = MpscChannel(net_clients.len() * 2);

    // Send the confs
    let mut handles = vec![];
    for (net_client, stake) in net_clients {
        let chunk = conf_chunk.clone();
        let mut chann_tx = conf_chann_tx.clone();
        handles.push(tokio::spawn(async move {
            let r = send_confs(chunk, net_client.clone(), conn).await;
            match chann_tx.send((r.0, stake)).await {
                Ok(_) => (),
                Err(e) => {
                    // Disconnect is okay since we may leave f running
                    if !e.is_disconnected() {
                        panic!("Send failed! {:?}", net_client)
                    }
                }
            }

            let _: Vec<_> =
                r.1.into_par_iter()
                    .map(check_transaction_response)
                    .collect();
        }));
    }
    drop(conf_chann_tx);

    // Reset counter
    total = 0;
    while let Some(v) = time::timeout(Duration::from_secs(10), conf_chann_rx.next())
        .await
        .unwrap()
    {
        total += v.1;
        if total >= quorum_threshold {
            break;
        }
    }
    if total < quorum_threshold {
        panic!("Quorum threshold not reached for confirmation")
    }

    let elapsed = time_start.elapsed().as_micros();

    // Send the total time over
    result_chann_tx.send(elapsed).await.unwrap();
}

pub struct MultiFixedRateLoadGenerator {
    /// The time between sending transactions chunks
    /// Anything below 10ms causes degradation in resolution
    pub period_us: u64,

    pub tick_notifier: Arc<Notify>,

    /// Number of TCP connections to open
    pub connections: usize,

    /// Transactions to be sent
    pub transactions: Vec<(Transaction, CertifiedTransaction)>,

    /// Results are sent over this channel
    pub results_chann_rx: Receiver<u128>,

    /// This is the chunk size actually assigned for each tick per task
    /// It is 2*chunk_size due to order and confirmation steps
    pub chunk_size_per_task: usize,
}

impl MultiFixedRateLoadGenerator {
    pub async fn new(
        transactions: Vec<(Transaction, CertifiedTransaction)>,
        period_us: u64,
        connections: usize,

        network_cfg: &NetworkConfig,
    ) -> Self {
        let network_clients_stake: Vec<(Multiaddr, StakeUnit)> = network_cfg
            .validator_set()
            .iter()
            .map(|q| (q.network_address().to_owned(), q.stake()))
            .collect();
        let committee_quorum_threshold = network_cfg.committee().quorum_threshold();
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
            let clients = network_clients_stake.clone();

            let mut order_chunk = vec![];
            let mut conf_chunk = vec![];

            for ch in tx_chunk {
                order_chunk.push(ch.0.clone());
                conf_chunk.push(ch.1.clone());
            }

            handles.push(tokio::spawn(async move {
                send_tx_for_quorum(
                    notif,
                    order_chunk,
                    conf_chunk,
                    &mut result_chann_tx,
                    clients,
                    conn,
                    committee_quorum_threshold,
                )
                .await;
            }));
        }

        drop(result_chann_tx);

        Self {
            period_us,
            transactions,
            connections,
            results_chann_rx,
            tick_notifier,
            chunk_size_per_task: num_chunks_per_task,
            //network_config: network_cfg.authorities,
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
