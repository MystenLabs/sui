// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_protocol_config::Chain;
use sui_protocol_config::ProtocolConfig;
use sui_protocol_config::ProtocolVersion;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetEpochRequest;
use sui_rpc::proto::sui::rpc::v2::GetProtocolConfigRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use test_cluster::TestClusterBuilder;

/// A setting that has no value at the first protocol version.
const UNSET_AT_MIN_VERSION: &str = "bridge_should_try_to_finalize_committee";

#[sim_test]
async fn get_protocol_config() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let current_version = client
        .get_epoch(
            GetEpochRequest::latest()
                .with_read_mask(FieldMask::from_paths(["protocol_config.protocol_version"])),
        )
        .await
        .unwrap()
        .into_inner()
        .epoch
        .unwrap()
        .protocol_config
        .unwrap()
        .protocol_version
        .unwrap();

    // Default version and default read mask: only the current protocol version.
    let config = client
        .get_protocol_config(GetProtocolConfigRequest::default())
        .await
        .unwrap()
        .into_inner()
        .protocol_config
        .unwrap();
    assert_eq!(config.protocol_version, Some(current_version));
    assert!(config.feature_flags.is_empty());
    assert!(config.attributes.is_empty());
    assert!(config.configs.is_empty());

    // Default version with every field matches the current version requested explicitly.
    let latest = client
        .get_protocol_config(
            GetProtocolConfigRequest::default().with_read_mask(FieldMask::from_str("*")),
        )
        .await
        .unwrap()
        .into_inner()
        .protocol_config
        .unwrap();
    let explicit_current = client
        .get_protocol_config(
            GetProtocolConfigRequest::default()
                .with_version(current_version)
                .with_read_mask(FieldMask::from_str("*")),
        )
        .await
        .unwrap()
        .into_inner()
        .protocol_config
        .unwrap();
    assert_eq!(latest, explicit_current);
    assert!(!latest.feature_flags.is_empty());

    // An explicit older version: settings unset at that version are absent.
    let min_version = ProtocolVersion::MIN.as_u64();
    let expected = ProtocolConfig::get_for_version(ProtocolVersion::MIN, Chain::Unknown);
    assert!(
        expected
            .bridge_should_try_to_finalize_committee_as_option()
            .is_none()
    );
    let old = client
        .get_protocol_config(
            GetProtocolConfigRequest::default()
                .with_version(min_version)
                .with_read_mask(FieldMask::from_str("*")),
        )
        .await
        .unwrap()
        .into_inner()
        .protocol_config
        .unwrap();
    assert_eq!(old.protocol_version, Some(min_version));
    assert_eq!(old.feature_flags, expected.feature_map());
    assert!(!old.configs.contains_key(UNSET_AT_MIN_VERSION));
    assert!(!old.attributes.contains_key(UNSET_AT_MIN_VERSION));
    for (name, enabled) in &old.feature_flags {
        assert_eq!(old.configs[name], prost_types::Value::from(*enabled));
    }

    // The read mask selects fields.
    let flags_only = client
        .get_protocol_config(
            GetProtocolConfigRequest::default()
                .with_version(min_version)
                .with_read_mask(FieldMask::from_paths(["feature_flags"])),
        )
        .await
        .unwrap()
        .into_inner()
        .protocol_config
        .unwrap();
    assert_eq!(flags_only.protocol_version, None);
    assert_eq!(flags_only.feature_flags, old.feature_flags);
    assert!(flags_only.configs.is_empty());
    assert!(flags_only.attributes.is_empty());

    let status = client
        .get_protocol_config(
            GetProtocolConfigRequest::default()
                .with_read_mask(FieldMask::from_paths(["not_a_field"])),
        )
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);

    // Versions the chain has not reached, and version 0, are not found.
    for version in [current_version + 1, 0] {
        let status = client
            .get_protocol_config(GetProtocolConfigRequest::default().with_version(version))
            .await
            .unwrap_err();
        assert_eq!(status.code(), tonic::Code::NotFound, "version {version}");
    }
}
