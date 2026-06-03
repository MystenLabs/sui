// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Minimal port of
//! `sui-e2e-tests/tests/rpc/v2/state_service/list_owned_objects.rs`.
//! That file's main scenario (`test_indexing_with_tto`) publishes
//! the on-disk `data/tto` Move package and executes a multi-step
//! TTO flow against a real fullnode — porting it requires
//! in-process Move build infrastructure the harness doesn't have
//! today. The cheaper coverage that fits the read-only model is
//! "after Simulacrum funds an account, the new gas coin shows up
//! in `list_owned_objects`", which exercises every CF the
//! [`object_by_owner`] pipeline writes.

use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

#[tokio::test]
async fn list_owned_objects_reports_funded_account_gas_coin() {
    let cluster = LocalCluster::new().await.unwrap();
    let (owner, _kp, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut client: StateServiceClient<Channel> =
        StateServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    let objects = client
        .list_owned_objects({
            let mut req = ListOwnedObjectsRequest::default();
            req.owner = Some(owner.to_string());
            req.read_mask = Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "object_type",
            ]));
            req
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    assert!(
        !objects.is_empty(),
        "freshly funded account should own at least its gas coin",
    );

    let gas_id = gas.0.to_string();
    let found = objects.iter().find(|o| o.object_id() == gas_id);
    assert!(
        found.is_some(),
        "the gas coin returned by funded_account should appear in list_owned_objects",
    );

    let found = found.unwrap();
    assert!(found.object_type.is_some());
    assert!(found.version.is_some());
    assert!(found.digest.is_some());
}
