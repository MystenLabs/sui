use crate::authority::test_and_configure_authority_configs;
use mysten_metrics::RegistryService;
use narwhal_config::SharedWorkerCache;
use narwhal_executor::ExecutionState;
use narwhal_types::ConsensusOutput;
use narwhal_worker::TrivialTransactionValidator;
use prometheus::Registry;
use std::sync::Arc;
use std::time::Duration;
use sui_core::narwhal_manager::{run_narwhal_manager, NarwhalConfiguration, NarwhalManager};
use sui_types::crypto::KeypairTraits;
use sui_types::messages::Transaction;
use tokio::sync::mpsc::channel;

#[derive(Clone)]
struct EpochAwareExecutionState {}

#[async_trait::async_trait]
impl ExecutionState for EpochAwareExecutionState {
    async fn handle_consensus_output(&self, _consensus_output: ConsensusOutput) {}

    async fn last_executed_sub_dag_index(&self) -> u64 {
        0
    }
}

impl EpochAwareExecutionState {
    #[allow(unused)]
    async fn process_transaction(&self, _transaction: Transaction) {
        //let _transaction: u64 = bincode::deserialize(&transaction).unwrap();
    }
}

#[tokio::test]
async fn test_narwhal_manager() {
    let configs = test_and_configure_authority_configs(1);
    let registry_service = RegistryService::new(Registry::new());

    let config = configs.validator_configs()[0].clone();
    let consensus_config = config.consensus_config().unwrap();

    let state = Arc::new(EpochAwareExecutionState {});

    let narwhal_config = NarwhalConfiguration {
        primary_keypair: config.protocol_key_pair().copy(),
        network_keypair: config.network_key_pair.copy(),
        worker_ids_and_keypairs: vec![(0, config.worker_key_pair().copy())],
        storage_base_path: consensus_config.db_path().to_path_buf(),
        parameters: consensus_config.narwhal_config().to_owned(),
        execution_state: state,
        tx_validator: TrivialTransactionValidator::default(),
        registry_service,
    };

    let (tx_start, tr_start) = channel(1);
    let (tx_stop, tr_stop) = channel(1);
    let join_handle = tokio::spawn(run_narwhal_manager(narwhal_config, tr_start, tr_stop));

    let narwhal_manager = NarwhalManager {
        join_handle,
        tx_start,
        tx_stop,
    };

    let mut committee = config.genesis().unwrap().narwhal_committee_inner();
    let mut worker_cache = config.narwhal_worker_cache_inner().unwrap();

    // start narwhal
    narwhal_manager
        .tx_start
        .send((
            Arc::new(committee.clone()),
            SharedWorkerCache::from(worker_cache.clone()),
        ))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // stop narwhal
    narwhal_manager.tx_stop.send(()).await.unwrap();

    // advance epoch
    committee.epoch = 1;
    worker_cache.epoch = 1;

    // start narwhal with advanced epoch
    narwhal_manager
        .tx_start
        .send((
            Arc::new(committee.clone()),
            SharedWorkerCache::from(worker_cache.clone()),
        ))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(30)).await;
}
