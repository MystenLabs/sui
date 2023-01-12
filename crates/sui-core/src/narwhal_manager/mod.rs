// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
#[path = "../unit_tests/narwhal_manager_tests.rs"]
pub mod narwhal_manager_tests;

use arc_swap::ArcSwap;
use fastcrypto::bls12381;
use fastcrypto::traits::KeyPair;
use mysten_metrics::RegistryService;
use narwhal_config::{Committee, Parameters, SharedWorkerCache, WorkerId};
use narwhal_executor::ExecutionState;
use narwhal_node::primary_node::PrimaryNode;
use narwhal_node::worker_node::WorkerNodes;
use narwhal_node::NodeStorage;
use narwhal_worker::TransactionValidator;
use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::sync::Arc;
use sui_types::crypto::NetworkKeyPair;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

pub struct NarwhalStartMessage<State> {
    pub committee: Arc<Committee>,
    pub shared_worker_cache: SharedWorkerCache,
    pub execution_state: Arc<State>,
}

impl<State> Debug for NarwhalStartMessage<State> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.committee.fmt(f)?;
        self.shared_worker_cache.fmt(f)
    }
}

pub struct NarwhalManager<State> {
    pub join_handle: JoinHandle<()>,
    pub tx_start: Sender<NarwhalStartMessage<State>>,
    pub tx_stop: Sender<()>,
}

pub struct NarwhalConfiguration<TxValidator: TransactionValidator> {
    pub primary_keypair: bls12381::min_sig::BLS12381KeyPair,
    pub network_keypair: NetworkKeyPair,
    pub worker_ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,

    pub storage_base_path: PathBuf,
    pub parameters: Parameters,
    pub tx_validator: TxValidator,

    pub registry_service: RegistryService,
}

pub async fn run_narwhal_manager<State, TxValidator>(
    config: NarwhalConfiguration<TxValidator>,
    mut tr_start: Receiver<NarwhalStartMessage<State>>,
    mut tr_stop: Receiver<()>,
) where
    State: ExecutionState + Send + Sync + 'static,
    TxValidator: TransactionValidator,
{
    loop {
        // Copy the config for this iteration of the loop
        let mut id_keypair_copy = Vec::new();
        for (id, keypair) in &config.worker_ids_and_keypairs {
            id_keypair_copy.push((*id, keypair.copy()));
        }

        // Wait for instruction to start an instance of narwhal
        let NarwhalStartMessage {
            committee,
            shared_worker_cache,
            execution_state,
        } = match tr_start.recv().await {
            Some(m) => m,
            None => break,
        };

        let config_copy = NarwhalConfiguration {
            primary_keypair: config.primary_keypair.copy(),
            network_keypair: config.network_keypair.copy(),
            worker_ids_and_keypairs: id_keypair_copy,
            storage_base_path: config.storage_base_path.clone(),
            parameters: config.parameters.clone(),
            tx_validator: config.tx_validator.clone(),
            registry_service: config.registry_service.clone(),
        };

        let (primary_node, worker_nodes) =
            start_narwhal(config_copy, committee, shared_worker_cache, execution_state).await;

        // Wait for instruction to stop the instance of narwhal
        match tr_stop.recv().await {
            Some(_) => shutdown_narwhal(primary_node, worker_nodes).await,
            None => break,
        };
    }
}

async fn shutdown_narwhal(primary_node: PrimaryNode, worker_nodes: WorkerNodes) {
    tracing::info!("Shutting down narwhal");

    primary_node.shutdown().await;
    worker_nodes.shutdown().await;

    tracing::info!("Narwhal shutdown is complete");
}

async fn start_narwhal<State, TxValidator>(
    config: NarwhalConfiguration<TxValidator>,
    committee: Arc<Committee>,
    worker_cache: SharedWorkerCache,
    execution_state: Arc<State>,
) -> (PrimaryNode, WorkerNodes)
where
    State: ExecutionState + Send + Sync + 'static,
    TxValidator: TransactionValidator,
{
    // Create a new store
    let mut store_path = config.storage_base_path.clone();
    store_path.push(format!("epoch{}", committee.epoch()));
    let store = NodeStorage::reopen(store_path);

    let name = config.primary_keypair.public().clone();

    tracing::info!("Starting up narwhal");

    // Start Narwhal Primary with configuration
    let primary_node = PrimaryNode::new(
        config.parameters.clone(),
        true,
        config.registry_service.clone(),
    );

    primary_node
        .start(
            config.primary_keypair.copy(),
            config.network_keypair.copy(),
            Arc::new(ArcSwap::new(committee.clone())),
            worker_cache.clone(),
            &store,
            execution_state,
        )
        .await
        .expect("Unable to start Narwhal Primary");

    // Start Narwhal Workers with configuration
    let worker_nodes = WorkerNodes::new(config.registry_service.clone(), config.parameters.clone());
    worker_nodes
        .start(
            name,
            config.worker_ids_and_keypairs,
            Arc::new(ArcSwap::new(committee.clone())),
            worker_cache,
            &store,
            config.tx_validator.clone(),
        )
        .await
        .expect("Unable to start Narwhal Worker");

    tracing::info!("Starting up narwhal is complete");

    (primary_node, worker_nodes)
}
