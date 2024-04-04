// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::{path::PathBuf, sync::Arc};

use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use consensus_config::{Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
use consensus_core::{CommitConsumer, CommitIndex, ConsensusAuthority, NetworkType, Round};
use fastcrypto::ed25519;
use mysten_metrics::{RegistryID, RegistryService};
use narwhal_executor::ExecutionState;
use prometheus::Registry;
use sui_config::NodeConfig;
use sui_types::{
    committee::EpochId, sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait,
};
use tokio::sync::{mpsc::unbounded_channel, Mutex};

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
    running: Mutex<Running>,
    metrics: ConsensusManagerMetrics,
    registry_service: RegistryService,
    authority: ArcSwapOption<(ConsensusAuthority, RegistryID)>,
    // Use a shared lazy mysticeti client so we can update the internal mysticeti
    // client that gets created for every new epoch.
    client: Arc<LazyMysticetiClient>,
    consensus_handler: ArcSwapOption<MysticetiConsensusHandler>,
}

impl MysticetiManager {
    /// NOTE: Mysticeti protocol key uses Ed25519 instead of BLS.
    /// But for security, the protocol keypair must be different from the network keypair.
    pub fn new(
        protocol_keypair: ed25519::Ed25519KeyPair,
        network_keypair: ed25519::Ed25519KeyPair,
        storage_base_path: PathBuf,
        metrics: ConsensusManagerMetrics,
        registry_service: RegistryService,
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
            consensus_handler: ArcSwapOption::empty(),
        }
    }

    #[allow(unused)]
    fn get_store_path(&self, epoch: EpochId) -> PathBuf {
        let mut store_path = self.storage_base_path.clone();
        store_path.push(format!("{}", epoch));
        store_path
    }
}

#[async_trait]
impl ConsensusManagerTrait for MysticetiManager {
    async fn start(
        &self,
        _config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_handler_initializer: ConsensusHandlerInitializer,
        tx_validator: SuiTxValidator,
    ) {
        let system_state = epoch_store.epoch_start_state();
        let committee: Committee = system_state.get_mysticeti_committee();
        let epoch = epoch_store.epoch();
        let protocol_config = epoch_store.protocol_config();
        let network_type = match std::env::var("CONSENSUS_NETWORK") {
            Ok(type_str) => {
                if type_str.to_lowercase() == "tonic" {
                    NetworkType::Tonic
                } else {
                    NetworkType::Anemo
                }
            }
            Err(_) => NetworkType::Anemo,
        };

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

        // TODO(mysticeti): Fill in the other fields
        let parameters = Parameters {
            db_path: Some(self.get_store_path(epoch)),
            ..Default::default()
        };

        let own_protocol_key = self.protocol_keypair.public();
        let (own_index, _) = committee
            .authorities()
            .find(|(_, a)| a.protocol_key == own_protocol_key)
            .expect("Own authority should be among the consensus authorities!");

        let registry = Registry::new_custom(Some("consensus".to_string()), None).unwrap();

        // TODO: that should be replaced by a metered channel. We can discuss if unbounded approach
        // is the one we want to go with.
        #[allow(clippy::disallowed_methods)]
        let (commit_sender, commit_receiver) = unbounded_channel();

        let consensus_handler = consensus_handler_initializer.new_consensus_handler();
        let consumer = CommitConsumer::new(
            commit_sender,
            // TODO(mysticeti): remove dependency on narwhal executor
            consensus_handler.last_executed_sub_dag_round() as Round,
            consensus_handler.last_executed_sub_dag_index() as CommitIndex,
        );

        // TODO(mysticeti): Investigate if we need to return potential errors from
        // AuthorityNode and add retries here?
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
        )
        .await;

        let registry_id = self.registry_service.add(registry.clone());

        self.authority
            .swap(Some(Arc::new((authority, registry_id))));

        // create the client to send transactions to Mysticeti and update it.
        self.client.set(
            self.authority
                .load()
                .as_ref()
                .expect("ConsensusAuthority should have been created by now.")
                .0
                .transaction_client(),
        );

        // spin up the new mysticeti consensus handler to listen for committed sub dags
        let handler = MysticetiConsensusHandler::new(consensus_handler, commit_receiver);
        self.consensus_handler.store(Some(Arc::new(handler)));
    }

    async fn shutdown(&self) {
        let Some(_guard) = RunningLockGuard::acquire_shutdown(&self.metrics, &self.running).await
        else {
            return;
        };

        // swap with empty to ensure there is no other reference to authority and we can safely do Arc unwrap
        let r = self.authority.swap(None).unwrap();
        let Ok((authority, registry_id)) = Arc::try_unwrap(r) else {
            panic!("Failed to retrieve the mysticeti authority");
        };

        // shutdown the authority and wait for it
        authority.stop().await;

        // drop the old consensus handler to force stop any underlying task running.
        self.consensus_handler.store(None);

        // unregister the registry id
        self.registry_service.remove(registry_id);
    }

    async fn is_running(&self) -> bool {
        Running::False != *self.running.lock().await
    }

    fn get_storage_base_path(&self) -> PathBuf {
        self.storage_base_path.clone()
    }
}
