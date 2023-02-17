// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::RegistryService;
use prometheus::Registry;
use sui_macros::*;
use sui_protocol_config::{ProtocolVersion, SupportedProtocolVersions};
use test_utils::authority::start_node;

#[sim_test]
#[should_panic]
async fn test_validator_panics_on_unsupported_protocol_version() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut network_config = sui_config::builder::ConfigBuilder::new(&dir)
        .with_protocol_version(ProtocolVersion::new(2))
        .build();
    network_config.validator_configs[0].supported_protocol_versions =
        Some(SupportedProtocolVersions::new_for_testing(1, 1));

    let registry_service = RegistryService::new(Registry::new());
    let _sui_node = start_node(&network_config.validator_configs[0], registry_service).await;
}
