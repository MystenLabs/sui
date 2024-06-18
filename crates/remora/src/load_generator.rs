// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{net::SocketAddr, time::Duration};

use bytes::Bytes;
use itertools::Itertools;
use network::SimpleSender;
use sui_single_node_benchmark::{
    benchmark_context::BenchmarkContext,
    command::{Component, WorkloadKind},
    workload::Workload,
};
use sui_types::transaction::CertifiedTransaction;
use tokio::time::{interval, Instant, MissedTickBehavior};

use crate::{
    metrics::{ErrorType, Metrics},
    types::TransactionWithEffects,
};

/// Default workload for the load generator.
const DEFAULT_WORKLOAD: WorkloadKind = WorkloadKind::PTB {
    num_transfers: 0,
    num_dynamic_fields: 0,
    use_batch_mint: false,
    computation: 0,
    use_native_transfer: false,
    num_mints: 0,
    num_shared_objects: 0,
    nft_size: 32,
};

/// The load generator generates transactions at a specified rate and submits them to the system.
pub struct LoadGenerator {
    /// Number of transactions per second to submit to the system.
    load: u64,
    /// Duration of the load test.
    duration: Duration,
    /// The network target to send transactions to.
    target: SocketAddr,
    /// A best effort network sender.
    network: SimpleSender,
    /// Metrics for the load generator.
    metrics: Metrics,
}

impl LoadGenerator {
    /// Create a new load generator.
    pub fn new(load: u64, duration: Duration, target: SocketAddr, metrics: Metrics) -> Self {
        LoadGenerator {
            load,
            duration,
            target,
            network: SimpleSender::new(),
            metrics,
        }
    }

    /// Initialize the load generator. This will generate all required genesis objects and all transactions upfront.
    // TODO: This may be problematic if the number of transactions is very large. We may need to
    // generate transactions on the fly.
    pub async fn initialize(&self) -> Vec<CertifiedTransaction> {
        let pre_generation = self.load * self.duration.as_secs();

        // Create genesis.
        tracing::debug!("Creating genesis for {pre_generation} transactions...");
        let start_time = Instant::now();
        let workload = Workload::new(pre_generation, DEFAULT_WORKLOAD);
        let component = Component::PipeTxsToChannel;
        let mut ctx = BenchmarkContext::new(workload.clone(), component, true).await;
        let elapsed = start_time.elapsed();
        tracing::debug!(
            "Genesis created {} accounts/s in {} ms",
            workload.num_accounts() as f64 / elapsed.as_secs_f64(),
            elapsed.as_millis(),
        );

        // Pre-generate all transactions.
        tracing::debug!("Generating all transactions...");
        let start_time = Instant::now();
        let tx_generator = workload.create_tx_generator(&mut ctx).await;
        let transactions = ctx.generate_transactions(tx_generator).await;
        let transactions = ctx.certify_transactions(transactions, false).await;
        let elapsed = start_time.elapsed();
        tracing::debug!(
            "Generated {} txs in {} ms",
            transactions.len(),
            elapsed.as_millis(),
        );

        transactions
    }

    /// Run the load generator. This will submit transactions to the system at the specified rate
    /// until all transactions are submitted.
    pub async fn run(&mut self, transactions: Vec<CertifiedTransaction>) {
        let precision = if self.load > 1000 { 20 } else { 1 };
        let burst_duration = Duration::from_millis(1000 / precision);
        let mut interval = interval(burst_duration);
        interval.set_missed_tick_behavior(MissedTickBehavior::Burst);

        let mut counter = 0;
        let chunks_size = self.load / precision;
        for chunk in &transactions.into_iter().chunks(chunks_size as usize) {
            if counter % 1000 == 0 && counter != 0 {
                tracing::debug!("Submitted {} txs", counter * chunks_size);
            }

            let now = Instant::now();
            let timestamp = Metrics::now().as_secs_f64();
            for tx in chunk {
                let full_tx = TransactionWithEffects {
                    tx,
                    ground_truth_effects: None,
                    child_inputs: None,
                    checkpoint_seq: None,
                    timestamp,
                };
                let bytes = bincode::serialize(&full_tx).expect("serialization failed");
                let address = self.target.clone();
                self.network.send(address, Bytes::from(bytes)).await;
            }

            if now.elapsed() > burst_duration {
                tracing::warn!("Transaction rate too high for this client");
                self.metrics
                    .register_error(ErrorType::TransactionRateTooHigh);
            }

            counter += 1;
            interval.tick().await;
        }
    }
}

#[cfg(test)]
mod test {
    use std::{net::SocketAddr, time::Duration};

    use bytes::Bytes;
    use futures::{sink::SinkExt, stream::StreamExt};
    use prometheus::Registry;
    use tokio::{net::TcpListener, task::JoinHandle};
    use tokio_util::codec::{Framed, LengthDelimitedCodec};

    use crate::{load_generator::LoadGenerator, metrics::Metrics, types::TransactionWithEffects};

    /// Create a network listener that will receive a single message and return it.
    fn listener(address: SocketAddr) -> JoinHandle<Bytes> {
        tokio::spawn(async move {
            let listener = TcpListener::bind(&address).await.unwrap();
            let (socket, _) = listener.accept().await.unwrap();
            let transport = Framed::new(socket, LengthDelimitedCodec::new());
            let (mut writer, mut reader) = transport.split();
            match reader.next().await {
                Some(Ok(received)) => {
                    writer.send(Bytes::from("Ack")).await.unwrap();
                    return received.freeze();
                }
                _ => panic!("Failed to receive network message"),
            }
        })
    }

    #[tokio::test]
    async fn generate_transactions() {
        // Boot a test server to receive transactions.
        // TODO: Implement a better way to get a port for tests
        let target = SocketAddr::from(([127, 0, 0, 1], 18181));
        let handle = listener(target);
        tokio::task::yield_now().await;

        // Create genesis and generate transactions.
        let metrics = Metrics::new(&Registry::new());
        let mut load_generator = LoadGenerator::new(1, Duration::from_secs(1), target, metrics);
        let transactions = load_generator.initialize().await;

        // Submit transactions to the server.
        let now = Metrics::now().as_secs_f64();
        load_generator.run(transactions).await;

        // Check that the transactions were received.
        let received = handle.await.unwrap();
        let transaction: TransactionWithEffects = bincode::deserialize(&received).unwrap();
        let timestamp = transaction.timestamp;
        assert!(timestamp > now);
    }
}
