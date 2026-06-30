// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors `sui-e2e-tests/tests/rpc/v2/ledger_service/get_epoch.rs`
//! (the first test). Drops the
//! `get_epoch_protocol_config_exposes_gasless_allowlist` test
//! because it requires `ProtocolConfig::apply_overrides_for_testing`
//! which doesn't work in our process-shared simulacrum.

use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetEpochRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

#[tokio::test]
async fn get_epoch() {
    let cluster = LocalCluster::new().await.unwrap();

    let mut client: LedgerServiceClient<Channel> =
        LedgerServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    let latest_epoch = client
        .get_epoch(GetEpochRequest::latest().with_read_mask(FieldMask::from_str("*")))
        .await
        .unwrap()
        .into_inner()
        .epoch
        .unwrap();

    let epoch_0 = client
        .get_epoch(GetEpochRequest::new(0).with_read_mask(FieldMask::from_str("*")))
        .await
        .unwrap()
        .into_inner()
        .epoch
        .unwrap();

    assert_eq!(latest_epoch.committee, epoch_0.committee);

    // The proto committee shape should round-trip through the
    // sdk-types value cleanly.
    sui_sdk_types::ValidatorCommittee::try_from(&latest_epoch.committee.unwrap()).unwrap();

    assert_eq!(epoch_0.epoch, Some(0));
    assert_eq!(epoch_0.first_checkpoint, Some(0));

    let epoch = client
        .get_epoch(
            GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"])),
        )
        .await
        .unwrap()
        .into_inner()
        .epoch
        .unwrap();
    assert!(epoch.system_state.is_some());
}
