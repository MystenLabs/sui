use std::{collections::BTreeMap, fs, io::BufReader, path::PathBuf, time::Duration};

use super::agents::*;
use crate::{
    metrics::{Measurement, Metrics},
    types::*,
};
use async_trait::async_trait;
use futures::future;
use tokio::{sync::mpsc, task::JoinHandle, time::sleep};
use std::sync::Arc;

use sui_single_node_benchmark::{
    benchmark_context::BenchmarkContext,
    command::{Component, WorkloadKind},
    mock_account::Account,
    workload::Workload,
};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    object::Object,
    transaction::Transaction,
};

pub const WORKLOAD: WorkloadKind = WorkloadKind::NoMove;
pub const COMPONENT: Component = Component::PipeTxsToChannel;

pub async fn generate_benchmark_ctx_workload(
    tx_count: u64,
    duration: Duration,
) -> (BenchmarkContext, Workload) {
    let workload = Workload::new(tx_count * duration.as_secs(), WORKLOAD);
    println!(
        "Setting up benchmark...{tx_count} txs per second for {} seconds",
        duration.as_secs()
    );
    let start_time = std::time::Instant::now();
    let ctx = BenchmarkContext::new(workload.clone(), COMPONENT, true).await;
    let elapsed = start_time.elapsed().as_millis() as f64;
    println!(
        "Benchmark setup finished in {}ms at a rate of {} accounts/s",
        elapsed,
        1000f64 * workload.num_accounts() as f64 / elapsed
    );
    (ctx, workload)
}

pub async fn generate_benchmark_txs(
    workload: Workload,
    mut ctx: BenchmarkContext,
) -> (BenchmarkContext, Vec<Transaction>) {
    let start_time = std::time::Instant::now();
    let tx_generator = workload.create_tx_generator(&mut ctx).await;
    let transactions = ctx.generate_transactions(tx_generator).await;
    let elapsed = start_time.elapsed().as_millis() as f64;
    println!(
        "{} txs generated in {}ms at a rate of {} TPS",
        transactions.len(),
        elapsed,
        1000f64 * workload.tx_count as f64 / elapsed,
    );

    (ctx, transactions)
}

/*****************************************************************************************
 *                                 Txn Generator Agent                                   *
 *****************************************************************************************/

pub struct TxnGenAgent {
    id: UniqueId,
    out_channel: mpsc::Sender<NetworkMessage>,
    attrs: GlobalConfig,
}

#[async_trait]
impl Agent<RemoraMessage> for TxnGenAgent {
    fn new(
        id: UniqueId,
        _in_channel: mpsc::Receiver<NetworkMessage>,
        out_channel: mpsc::Sender<NetworkMessage>,
        attrs: GlobalConfig,
        // _metrics: Arc<Metrics>,
    ) -> Self {
        TxnGenAgent {
            id,
            out_channel,
            attrs,
        }
    }

    async fn run(&mut self) {
        println!("Starting TxnGen agent {}", self.id);
    }
}
