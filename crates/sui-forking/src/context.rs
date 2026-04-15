// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use rand::rngs::OsRng;
use tokio::sync::RwLock;

use simulacrum::Simulacrum;
use sui_protocol_config::Chain;

use crate::store::DataStore;

/// Shared context for the forked network, holding the simulacrum instance and metadata.
pub struct Context {
    simulacrum: Arc<RwLock<Simulacrum<OsRng, DataStore>>>,
    /// A clone of the `DataStore` handed to the simulacrum. Kept here so the
    /// RPC server can implement `RpcStateReader` against it without going
    /// through the `RwLock` that protects the simulacrum.
    data_store: DataStore,
    chain_identifier: Chain,
}

impl Context {
    pub(crate) fn new(
        simulacrum: Simulacrum<OsRng, DataStore>,
        data_store: DataStore,
        chain_identifier: Chain,
    ) -> Self {
        Self {
            simulacrum: Arc::new(RwLock::new(simulacrum)),
            data_store,
            chain_identifier,
        }
    }

    pub(crate) fn simulacrum(&self) -> &Arc<RwLock<Simulacrum<OsRng, DataStore>>> {
        &self.simulacrum
    }

    pub fn data_store(&self) -> &DataStore {
        &self.data_store
    }

    pub(crate) fn chain_identifier(&self) -> &Chain {
        &self.chain_identifier
    }
}
