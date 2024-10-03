// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Args;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr};
use std::{env, net::IpAddr};
use sui_json_rpc::name_service::NameServiceConfig;
use sui_types::base_types::{ObjectID, SuiAddress};

/// config as loaded from `config.yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IndexerConfig {
    pub remote_store_url: String,
    #[serde(default = "default_db_url")]
    pub db_url: String,
    /// Only provide this if you use a colocated FN
    pub checkpoints_path: Option<String>,
    pub sui_rpc_url: String,
    pub deepbook_package_id: String,
    pub deepbook_genesis_checkpoint: u64,
    pub concurrency: u64,
    pub metric_port: u16,
    pub resume_from_checkpoint: Option<u64>,
    #[serde(default)]
    pub json_rpc_config: JsonRpcConfig,
}

impl sui_config::Config for IndexerConfig {}

pub fn default_db_url() -> String {
    env::var("DB_URL").expect("db_url must be set in config or via the $DB_URL env var")
}

#[derive(Args, Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcConfig {
    #[command(flatten)]
    pub name_service_options: NameServiceOptions,

    #[clap(long, default_value = "0.0.0.0:9000")]
    pub rpc_address: SocketAddr,
}

impl Default for JsonRpcConfig {
    fn default() -> Self {
        Self {
            name_service_options: NameServiceOptions::default(),
            rpc_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9000),
        }
    }
}

#[derive(Args, Debug, Clone, Deserialize, Serialize)]
pub struct NameServiceOptions {
    #[arg(default_value_t = NameServiceConfig::default().package_address)]
    #[arg(long = "name-service-package-address")]
    pub package_address: SuiAddress,
    #[arg(default_value_t = NameServiceConfig::default().registry_id)]
    #[arg(long = "name-service-registry-id")]
    pub registry_id: ObjectID,
    #[arg(default_value_t = NameServiceConfig::default().reverse_registry_id)]
    #[arg(long = "name-service-reverse-registry-id")]
    pub reverse_registry_id: ObjectID,
}

impl NameServiceOptions {
    pub fn to_config(&self) -> NameServiceConfig {
        let Self {
            package_address,
            registry_id,
            reverse_registry_id,
        } = self.clone();
        NameServiceConfig {
            package_address,
            registry_id,
            reverse_registry_id,
        }
    }
}

impl Default for NameServiceOptions {
    fn default() -> Self {
        let NameServiceConfig {
            package_address,
            registry_id,
            reverse_registry_id,
        } = NameServiceConfig::default();
        Self {
            package_address,
            registry_id,
            reverse_registry_id,
        }
    }
}
