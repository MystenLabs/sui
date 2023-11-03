// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_handler::ConsensusHandlerInitializer;
use crate::consensus_manager::ConsensusManagerTrait;
use crate::consensus_validator::SuiTxValidator;
use async_trait::async_trait;
use narwhal_config::Epoch;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::NodeConfig;

#[allow(unused)]
pub struct MysticetiManager {
    storage_base_path: PathBuf,
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
        todo!()
    }

    async fn shutdown(&self) {
        todo!()
    }

    fn get_storage_base_path(&self) -> PathBuf {
        self.storage_base_path.clone()
    }
}
