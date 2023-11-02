// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_handler::ConsensusHandlerInitializer;
use crate::consensus_manager::{
    ConsensusManagerMetrics, ConsensusManagerTrait, Running, RunningLockGuard,
};
use crate::consensus_validator::SuiTxValidator;
use async_trait::async_trait;
use narwhal_config::Epoch;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::NodeConfig;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use tokio::sync::Mutex;

#[allow(unused)]
pub struct MysticetiManager {
    storage_base_path: PathBuf,
    running: Mutex<Running>,
    metrics: ConsensusManagerMetrics,
}

impl MysticetiManager {
    #[allow(unused)]
    fn get_store_path(&self, epoch: Epoch) -> PathBuf {
        let mut store_path = self.storage_base_path.clone();
        store_path.push(format!("{}", epoch));
        store_path
    }
}

#[async_trait]
impl ConsensusManagerTrait for MysticetiManager {
    #[allow(unused)]
    async fn start(
        &self,
        config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_handler_initializer: ConsensusHandlerInitializer,
        tx_validator: SuiTxValidator,
    ) {
        let system_state = epoch_store.epoch_start_state();
        let committee = system_state.get_narwhal_committee();
        let protocol_config = epoch_store.protocol_config();

        let Some(mut guard) = RunningLockGuard::acquire_start(
            &self.metrics,
            &self.running,
            committee.epoch(),
            protocol_config.version,
        )
        .await
        else {
            return;
        };

        /*
        TODO: put validator bootstrap logic here
        */
        guard.completed();
    }

    async fn shutdown(&self) {
        let Some(mut guard) =
            RunningLockGuard::acquire_shutdown(&self.metrics, &self.running).await
        else {
            return;
        };

        /*
        TODO: put validator shutdown logic here
        */

        guard.completed();
    }

    async fn is_running(&self) -> bool {
        todo!()
    }

    fn get_storage_base_path(&self) -> PathBuf {
        self.storage_base_path.clone()
    }
}
