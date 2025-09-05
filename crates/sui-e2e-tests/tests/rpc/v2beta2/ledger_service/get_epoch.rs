// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2beta2::GetEpochRequest;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_epoch() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let latest_epoch = client
        .get_epoch(GetEpochRequest {
            epoch: None,
            read_mask: None,
        })
        .await
        .unwrap()
        .into_inner()
        .epoch
        .unwrap();

    let epoch_0 = client
        .get_epoch(GetEpochRequest {
            epoch: Some(0),
            read_mask: None,
        })
        .await
        .unwrap()
        .into_inner()
        .epoch
        .unwrap();

    assert_eq!(latest_epoch.committee, epoch_0.committee);

    // ensure we can convert proto committee type to sdk_types committee
    sui_sdk_types::ValidatorCommittee::try_from(&latest_epoch.committee.unwrap()).unwrap();

    assert_eq!(epoch_0.epoch, Some(0));
    assert_eq!(epoch_0.first_checkpoint, Some(0));

    //Ensure that fetching the system state for the epoch works
    let epoch = client
        .get_epoch(GetEpochRequest {
            epoch: None,
            read_mask: Some(FieldMask::from_paths(["system_state"])),
        })
        .await
        .unwrap()
        .into_inner()
        .epoch
        .unwrap();
    assert!(epoch.system_state.is_some());
}
