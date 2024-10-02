// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use sui_config::Config;
use sui_types::base_types::ObjectID;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DataSourceConfig {
    pub url: String,
    pub json_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct UploadFeedConfig {
    pub submission_interval: Duration,
    pub data_source_config: DataSourceConfig,
    pub upload_parameters: UploadParameters,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct UploadParameters {
    pub write_package_id: ObjectID,
    pub write_module_name: String,
    pub write_function_name: String,
    pub write_data_provider_object_id: ObjectID,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DownloadFeedConfigs {
    pub read_interval: Option<Duration>,
    pub read_feeds: HashMap<String, ObjectID>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct OracleNodeConfig {
    pub gas_object_id: ObjectID,
    pub upload_feeds: HashMap<String, HashMap<String, UploadFeedConfig>>,
    pub download_feeds: DownloadFeedConfigs,

    #[serde(default = "default_metrics_address")]
    pub metrics_address: SocketAddr,
}

fn default_metrics_address() -> SocketAddr {
    use std::net::{IpAddr, Ipv4Addr};
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9400)
}

impl Config for OracleNodeConfig {}
