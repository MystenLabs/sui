use std::time::Duration;
use super::agents::*;
use crate::{
    input_traffic_manager::input_traffic_manager_run,
    mock_consensus_worker::mock_consensus_worker_run, 
    primary_worker::{self},
    tx_gen_agent::WORKLOAD,
    types::*,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;
use sui_single_node_benchmark::{
    benchmark_context::BenchmarkContext, command::Component, workload::Workload,
};

pub struct PrimaryAgent {
    id: UniqueId,
    in_channel: mpsc::Receiver<NetworkMessage>,
    out_channel: mpsc::Sender<NetworkMessage>,
    attrs: GlobalConfig,
    // metrics: Arc<Metrics>,
}

pub const COMPONENT: Component = Component::Baseline;

#[async_trait]
impl Agent<RemoraMessage> for PrimaryAgent {
    fn new(
        id: UniqueId,
        in_channel: mpsc::Receiver<NetworkMessage>,
        out_channel: mpsc::Sender<NetworkMessage>,
        attrs: GlobalConfig,
        // _metrics: Arc<Metrics>,
    ) -> Self {
        PrimaryAgent {
            id,
            in_channel,
            out_channel,
            attrs,
        }
    }

    async fn run(&mut self) {
        println!("Starting Primary agent {}", self.id);

        let my_attrs = &self.attrs.get(&self.id).unwrap().attrs;
        let tx_count = my_attrs.get("tx_count").unwrap().parse().unwrap();

        let duration_secs = my_attrs["duration"].parse::<u64>().unwrap();
        let duration = Duration::from_secs(duration_secs);

        let workload = Workload::new(tx_count * duration.as_secs(), WORKLOAD);
        let context: Arc<BenchmarkContext> = {
            let ctx = BenchmarkContext::new(workload.clone(), COMPONENT, true).await;
            Arc::new(ctx)
        };

        let store = context.validator().create_in_memory_store();

        let (input_consensus_sender, mut input_consensus_receiver) =
            mpsc::unbounded_channel::<RemoraMessage>();
        let (input_executor_sender, mut input_executor_receiver) =
            mpsc::unbounded_channel::<RemoraMessage>();
        let (consensus_executor_sender, mut consensus_executor_receiver) =
            mpsc::unbounded_channel::<Vec<TransactionWithEffects>>();

        let mut primary_worker_state = primary_worker::PrimaryWorkerState::new(
            store,
            context.clone(),
        );

        let id = self.id.clone();
        let out_channel = self.out_channel.clone();

        tokio::spawn(async move {
            primary_worker_state
                .run(
                    tx_count,
                    duration,
                    &mut input_executor_receiver,
                    &mut consensus_executor_receiver,
                    &out_channel,
                    id,
                )
                .await;
        });

        tokio::spawn(async move {
            mock_consensus_worker_run(
                &mut input_consensus_receiver,
                &consensus_executor_sender,
                id,
            )
            .await;
        });

        {
            input_traffic_manager_run(
                &mut self.in_channel,
                &input_consensus_sender,
                &input_executor_sender,
                id,
            )
            .await;
        }
    }
}
