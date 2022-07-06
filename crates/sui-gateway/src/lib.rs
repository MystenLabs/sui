// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::GatewayConfig;
use anyhow::anyhow;
use std::path::Path;
use std::sync::Arc;
use sui_config::PersistedConfig;
use sui_core::gateway_state::{GatewayClient, GatewayMetrics, GatewayState};

pub mod config;
pub mod rpc_gateway_client;

pub fn create_client(
    config_path: &Path,
    gateway_metrics: GatewayMetrics,
) -> Result<GatewayClient, anyhow::Error> {
    let config: GatewayConfig = PersistedConfig::read(config_path).map_err(|e| {
        anyhow!(
            "Failed to read config file at {:?}: {}. Have you run `sui genesis` first?",
            config_path,
            e
        )
    })?;
    let committee = config.make_committee()?;
    let authority_clients = config.make_authority_clients();
    Ok(Arc::new(GatewayState::new(
        config.db_folder_path,
        committee,
        authority_clients,
        gateway_metrics,
    )?))
}
