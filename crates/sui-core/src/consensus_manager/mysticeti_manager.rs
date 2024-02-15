// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::{path::PathBuf, sync::Arc};

use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use consensus_config::{AuthorityIndex, Committee, Parameters};
use consensus_core::{CommitConsumer, ConsensusAuthority};
use fastcrypto::traits::KeyPair;
use mysten_metrics::{RegistryID, RegistryService};
use narwhal_executor::ExecutionState;
use prometheus::Registry;
use sui_config::NodeConfig;
use sui_types::{
    base_types::AuthorityName,
    committee::EpochId,
    crypto::{AuthorityKeyPair, NetworkKeyPair},
    sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait,
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
    keypair: AuthorityKeyPair,
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
    pub fn new(
        keypair: AuthorityKeyPair,
        network_keypair: NetworkKeyPair,
        storage_base_path: PathBuf,
        metrics: ConsensusManagerMetrics,
        registry_service: RegistryService,
        client: Arc<LazyMysticetiClient>,
    ) -> Self {
        Self {
            keypair,
            network_keypair,
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

        let name: AuthorityName = self.keypair.public().into();

        let authority_index: AuthorityIndex = committee
            .to_authority_index(
                epoch_store
                    .committee()
                    .authority_index(&name)
                    .expect("Should have valid index for own authority") as usize,
            )
            .expect("Should have valid index for own authority");

        let registry = Registry::new_custom(Some("mysticeti_".to_string()), None).unwrap();

        // TODO: that should be replaced by a metered channel. We can discuss if unbounded approach
        // is the one we want to go with.
        #[allow(clippy::disallowed_methods)]
        let (commit_sender, commit_receiver) = unbounded_channel();

        let consensus_handler = consensus_handler_initializer.new_consensus_handler();
        let consumer = CommitConsumer::new(
            commit_sender,
            // TODO(mysticeti): remove dependency on narwhal executor
            consensus_handler.last_executed_sub_dag_index().await,
        );

        // TODO(mysticeti): Investigate if we need to return potential errors from
        // AuthorityNode and add retries here?
        let authority = ConsensusAuthority::start(
            authority_index,
            committee.clone(),
            parameters.clone(),
            protocol_config.clone(),
            self.keypair.copy(),
            self.network_keypair.copy(),
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
