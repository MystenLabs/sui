// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::{path::PathBuf, sync::Arc, time::Duration};

use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use consensus_config::{Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
use consensus_core::{CommitConsumer, CommitIndex, ConsensusAuthority};
use fastcrypto::ed25519;
use mysten_metrics::{monitored_mpsc::unbounded_channel, RegistryID, RegistryService};
use prometheus::Registry;
use sui_config::NodeConfig;
use sui_protocol_config::ConsensusNetwork;
use sui_types::{
    committee::EpochId, sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait,
};
use tokio::sync::Mutex;
use tracing::info;

use crate::{
    authority::authority_per_epoch_store::AuthorityPerEpochStore,
    consensus_handler::{ConsensusHandlerInitializer, MysticetiConsensusHandler},
    consensus_manager::{
        ConsensusManagerMetrics, ConsensusManagerTrait, Running, RunningLockGuard,
    },
    consensus_validator::SuiTxValidator,
    mysticeti_adapter::LazyMysticetiClient,
};

#[cfg(test)]
#[path = "../unit_tests/mysticeti_manager_tests.rs"]
pub mod mysticeti_manager_tests;

pub struct MysticetiManager {
    protocol_keypair: ProtocolKeyPair,
    network_keypair: NetworkKeyPair,
    storage_base_path: PathBuf,
    // TODO: switch to parking_lot::Mutex.
    running: Mutex<Running>,
    metrics: Arc<ConsensusManagerMetrics>,
    registry_service: RegistryService,
    authority: ArcSwapOption<(ConsensusAuthority, RegistryID)>,
    boot_counter: Mutex<u64>,
    // Use a shared lazy mysticeti client so we can update the internal mysticeti
    // client that gets created for every new epoch.
    client: Arc<LazyMysticetiClient>,
    // TODO: switch to parking_lot::Mutex.
    consensus_handler: Mutex<Option<MysticetiConsensusHandler>>,
}

impl MysticetiManager {
    /// NOTE: Mysticeti protocol key uses Ed25519 instead of BLS.
    /// But for security, the protocol keypair must be different from the network keypair.
    pub fn new(
        protocol_keypair: ed25519::Ed25519KeyPair,
        network_keypair: ed25519::Ed25519KeyPair,
        storage_base_path: PathBuf,
        registry_service: RegistryService,
        metrics: Arc<ConsensusManagerMetrics>,
        client: Arc<LazyMysticetiClient>,
    ) -> Self {
        Self {
            protocol_keypair: ProtocolKeyPair::new(protocol_keypair),
            network_keypair: NetworkKeyPair::new(network_keypair),
            storage_base_path,
            running: Mutex::new(Running::False),
            metrics,
            registry_service,
            authority: ArcSwapOption::empty(),
            client,
            consensus_handler: Mutex::new(None),
            boot_counter: Mutex::new(0),
        }
    }

    fn get_store_path(&self, epoch: EpochId) -> PathBuf {
        let mut store_path = self.storage_base_path.clone();
        store_path.push(format!("{}", epoch));
        store_path
    }

    fn pick_network(&self, epoch_store: &AuthorityPerEpochStore) -> ConsensusNetwork {
        if let Ok(type_str) = std::env::var("CONSENSUS_NETWORK") {
            match type_str.to_lowercase().as_str() {
                "anemo" => return ConsensusNetwork::Anemo,
                "tonic" => return ConsensusNetwork::Tonic,
                _ => {
                    info!(
                        "Invalid consensus network type {} in env var. Continue to use the value from protocol config.",
                        type_str
                    );
                }
            }
        }
        epoch_store.protocol_config().consensus_network()
    }
}

#[async_trait]
impl ConsensusManagerTrait for MysticetiManager {
    async fn start(
        &self,
        config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_handler_initializer: ConsensusHandlerInitializer,
        tx_validator: SuiTxValidator,
    ) {
        let system_state = epoch_store.epoch_start_state();
        let committee: Committee = system_state.get_consensus_committee();
        let epoch = epoch_store.epoch();
        let protocol_config = epoch_store.protocol_config();
        let network_type = self.pick_network(&epoch_store);

        let Some(_guard) = RunningLockGuard::acquire_start(
            &self.metrics,
            &self.running,
            epoch,
            protocol_config.version,
        )
        .await
        else {
            return;
        };

        let consensus_config = config
            .consensus_config()
            .expect("consensus_config should exist");

        let mut parameters = Parameters {
            db_path: self.get_store_path(epoch),
            ..consensus_config.parameters.clone().unwrap_or_default()
        };

        // Disable the automated last known block sync for mainnet for now
        if epoch_store.get_chain_identifier().chain() == sui_protocol_config::Chain::Mainnet {
            parameters.sync_last_known_own_block_timeout = Duration::ZERO;
        };

        let own_protocol_key = self.protocol_keypair.public();
        let (own_index, _) = committee
            .authorities()
            .find(|(_, a)| a.protocol_key == own_protocol_key)
            .expect("Own authority should be among the consensus authorities!");

        let registry = Registry::new_custom(Some("consensus".to_string()), None).unwrap();

        let (commit_sender, commit_receiver) = unbounded_channel("consensus_output");

        let consensus_handler = consensus_handler_initializer.new_consensus_handler();
        let consumer = CommitConsumer::new(
            commit_sender,
            consensus_handler.last_processed_subdag_index() as CommitIndex,
        );
        let monitor = consumer.monitor();

        // TODO(mysticeti): Investigate if we need to return potential errors from
        // AuthorityNode and add retries here?
        let boot_counter = *self.boot_counter.lock().await;
        let authority = ConsensusAuthority::start(
            network_type,
            own_index,
            committee.clone(),
            parameters.clone(),
            protocol_config.clone(),
            self.protocol_keypair.clone(),
            self.network_keypair.clone(),
            Arc::new(tx_validator.clone()),
            consumer,
            registry.clone(),
            boot_counter,
        )
        .await;
        let client = authority.transaction_client();

        // Now increment the boot counter
        let mut boot_counter = self.boot_counter.lock().await;
        *boot_counter += 1;

        let registry_id = self.registry_service.add(registry.clone());

        let registered_authority = Arc::new((authority, registry_id));
        self.authority.swap(Some(registered_authority.clone()));

        // Initialize the client to send transactions to this Mysticeti instance.
        self.client.set(client);

        // spin up the new mysticeti consensus handler to listen for committed sub dags
        let handler = MysticetiConsensusHandler::new(consensus_handler, commit_receiver, monitor);
        let mut consensus_handler = self.consensus_handler.lock().await;
        *consensus_handler = Some(handler);

        // Wait until all locally available commits have been processed
        registered_authority.0.replay_complete().await;
    }

    async fn shutdown(&self) {
        let Some(_guard) = RunningLockGuard::acquire_shutdown(&self.metrics, &self.running).await
        else {
            return;
        };

        // Stop consensus submissions.
        self.client.clear();

        // swap with empty to ensure there is no other reference to authority and we can safely do Arc unwrap
        let r = self.authority.swap(None).unwrap();
        let Ok((authority, registry_id)) = Arc::try_unwrap(r) else {
            panic!("Failed to retrieve the mysticeti authority");
        };

        // shutdown the authority and wait for it
        authority.stop().await;

        // drop the old consensus handler to force stop any underlying task running.
        let mut consensus_handler = self.consensus_handler.lock().await;
        if let Some(mut handler) = consensus_handler.take() {
            handler.abort().await;
        }

        // unregister the registry id
        self.registry_service.remove(registry_id);
    }

    async fn is_running(&self) -> bool {
        Running::False != *self.running.lock().await
    }
}
