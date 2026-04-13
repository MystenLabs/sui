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
    chain_identifier: Chain,
}

impl Context {
    pub(crate) fn new(simulacrum: Simulacrum<OsRng, DataStore>, chain_identifier: Chain) -> Self {
        Self {
            simulacrum: Arc::new(RwLock::new(simulacrum)),
            chain_identifier,
        }
    }

    pub(crate) fn simulacrum(&self) -> &Arc<RwLock<Simulacrum<OsRng, DataStore>>> {
        &self.simulacrum
    }

    pub(crate) fn chain_identifier(&self) -> &Chain {
        &self.chain_identifier
    }
}
