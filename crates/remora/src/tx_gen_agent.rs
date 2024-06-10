use std::{collections::BTreeMap, fs, io::BufReader, path::PathBuf, time::Duration};

use super::agents::*;
use crate::{
    metrics::{Measurement, Metrics},
    types::*,
};
use async_trait::async_trait;
use futures::future;
use tokio::{
    sync::{mpsc, watch},
    time::{MissedTickBehavior, sleep},
    task::JoinHandle,
};
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
    transaction::{Transaction, CertifiedTransaction},
};

pub const WORKLOAD: WorkloadKind = WorkloadKind::PTB{
    num_transfers: 0, 
    num_dynamic_fields: 0,
    use_batch_mint: false,
    computation: 0,
    use_native_transfer: false,
    num_mints: 0,
    num_shared_objects: 0,
    nft_size: 32,
};
// pub const WORKLOAD: WorkloadKind = WorkloadKind::NoMove;
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
    // let skip_signing = false;
    // let transactions = ctx.certify_transactions(transactions, skip_signing).await;

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

impl TxnGenAgent {
    pub async fn run_inner
    (
        out_to_network: &mpsc::Sender<NetworkMessage>,
        tx_count: u64,
        duration: Duration,) 
    {
        let (ctx, workload) = generate_benchmark_ctx_workload(tx_count, duration).await;
        let (_, transactions) = generate_benchmark_txs(workload, ctx).await;
        
        const PRECISION: u64 = 20;
        let burst_duration = 1000 / PRECISION;
        let chunks_size = (tx_count / PRECISION) as usize;
        let mut counter = 0;
        let mut interval = tokio::time::interval(Duration::from_millis(burst_duration));
        interval.set_missed_tick_behavior(MissedTickBehavior::Burst);

        // Ugly - wait for EWs to finish generating genesis objects.
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Send transactions.
        println!("Starting benchmark");
        for chunk in transactions.chunks(chunks_size) {
            if counter % 1000 == 0 && counter != 0 {
                tracing::debug!("Submitted {} txs", counter * chunks_size);
            }
            for tx in chunk {
                let now = Metrics::now().as_secs_f64();
                let full_tx = TransactionWithEffects {
                    tx: tx.clone(),
                    ground_truth_effects: None,
                    child_inputs: None,
                    checkpoint_seq: None,
                    timestamp: now,
                };

                out_to_network
                    .send(NetworkMessage {
                        src: 0,
                        dst: vec![1,2],//get_ews_for_tx(&full_tx, &ew_ids).into_iter().collect(),
                        payload: RemoraMessage::ProposeExec(full_tx.clone()),
                    })
                    .await
                    .expect("sending failed");
            }
            counter += 1;
            interval.tick().await;
        }
        println!("[SW] Benchmark terminated");
    }
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

    async fn run(&mut self)
    {
        println!("Starting TxnGen agent {}", self.id);

        // Periodically print metrics
        let configs = self.attrs.clone();
        let workload = "default".to_string();
        let print_period = Duration::from_secs(10);
        // let _handle = Self::periodically_print_metrics(configs, workload, print_period);

        // Run Sequence Worker asynchronously
        let my_attrs = &self.attrs.get(&self.id).unwrap().attrs;
        let tx_count = my_attrs["tx_count"].parse::<u64>().unwrap();
        let duration_secs = my_attrs["duration"].parse::<u64>().unwrap();
        let duration = Duration::from_secs(duration_secs);
        // let working_dir = my_attrs
        //     .get("working_dir")
        //     .map_or("", String::as_str)
        //     .parse::<PathBuf>()
        //     .unwrap();
        TxnGenAgent::run_inner(
            &self.out_channel,
            tx_count,
            duration,
        )
        .await;
        println!("Txn Gen finished");

        loop {
            sleep(Duration::from_millis(1_000)).await;
        }
    }
  
}
