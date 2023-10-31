// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use async_trait::async_trait;
use narwhal_executor::ExecutionState;
use narwhal_worker::TransactionValidator;
use std::sync::Arc;
use sui_config::NodeConfig;

pub mod narwhal_manager;

/// Any consensus engine manager should implement this interface to handle via the sui node.
#[async_trait]
pub trait ConsensusManager {
    async fn start<State, StateInitializer, TxValidator: TransactionValidator>(
        &self,
        config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        execution_state: StateInitializer,
        tx_validator: TxValidator,
    ) where
        State: ExecutionState + Send + Sync + 'static,
        StateInitializer: Fn() -> State + Send + Sync;

    async fn shutdown(&self);
}
