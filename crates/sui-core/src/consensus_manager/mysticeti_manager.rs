// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use narwhal_config::Epoch;
use narwhal_executor::ExecutionState;
use narwhal_worker::TransactionValidator;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::NodeConfig;

#[allow(unused)]
pub struct MysticetiManager {
    storage_base_path: PathBuf,
}

impl MysticetiManager {
    #[allow(unused)]
    pub async fn start<State, StateInitializer, TxValidator: TransactionValidator>(
        &self,
        config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        execution_state: StateInitializer,
        tx_validator: TxValidator,
    ) where
        State: ExecutionState + Send + Sync + 'static,
        StateInitializer: Fn() -> State + Send + Sync,
    {
        todo!()
    }

    pub async fn shutdown(&self) {
        todo!()
    }

    pub fn get_storage_base_path(&self) -> PathBuf {
        self.storage_base_path.clone()
    }

    #[allow(unused)]
    fn get_store_path(&self, epoch: Epoch) -> PathBuf {
        let mut store_path = self.storage_base_path.clone();
        store_path.push(format!("{}", epoch));
        store_path
    }
}
