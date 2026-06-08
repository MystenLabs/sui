// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors `sui-e2e-tests/tests/rpc/v2/ledger_service/get_object.rs`.
//! Exercises field-mask defaults vs. explicit read-mask behaviour
//! on `get_object` and the parallel shape of `batch_get_objects`.

use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_sdk_types::Address;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

#[tokio::test]
async fn get_object() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut client: LedgerServiceClient<Channel> =
        LedgerServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    let id: Address = "0x5".parse().unwrap();

    // Request with no provided read_mask — defaults to id /
    // version / digest only.
    let Object {
        bcs,
        object_id,
        version,
        digest,
        owner,
        object_type,
        has_public_transfer,
        contents,
        package,
        previous_transaction,
        storage_rebate,
        json,
        ..
    } = client
        .get_object(GetObjectRequest::new(&id))
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();

    assert_eq!(object_id, Some(id.to_string()));
    assert!(version.is_some());
    assert!(digest.is_some());
    assert!(owner.is_none());
    assert!(bcs.is_none());
    assert!(object_type.is_none());
    assert!(has_public_transfer.is_none());
    assert!(contents.is_none());
    assert!(package.is_none());
    assert!(previous_transaction.is_none());
    assert!(storage_rebate.is_none());
    assert!(json.is_none());

    // Request with an explicit read_mask, asking for `object_id`
    // and `version` only.
    let Object {
        bcs,
        object_id,
        version,
        digest,
        owner,
        object_type,
        has_public_transfer,
        contents,
        package,
        previous_transaction,
        storage_rebate,
        json,
        ..
    } = client
        .get_object(
            GetObjectRequest::new(&id)
                .with_version(1u64)
                .with_read_mask(FieldMask::from_str("object_id,version")),
        )
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();

    assert_eq!(object_id, Some(id.to_string()));
    assert_eq!(version, Some(1));
    assert!(digest.is_none());
    assert!(owner.is_none());
    assert!(bcs.is_none());
    assert!(object_type.is_none());
    assert!(has_public_transfer.is_none());
    assert!(contents.is_none());
    assert!(package.is_none());
    assert!(previous_transaction.is_none());
    assert!(storage_rebate.is_none());
    assert!(json.is_none());

    // Request bcs + json, but not the secondary metadata fields.
    let Object {
        bcs,
        object_id,
        version,
        digest,
        owner,
        object_type,
        has_public_transfer,
        contents,
        package,
        previous_transaction,
        storage_rebate,
        json,
        ..
    } = client
        .get_object(
            GetObjectRequest::new(&id).with_read_mask(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "bcs",
                "json",
            ])),
        )
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();

    assert!(object_id.is_some());
    assert!(version.is_some());
    assert!(digest.is_some());
    assert!(bcs.is_some());
    assert!(owner.is_none());
    assert!(object_type.is_none());
    assert!(has_public_transfer.is_none());
    assert!(contents.is_none());
    assert!(package.is_none());
    assert!(previous_transaction.is_none());
    assert!(storage_rebate.is_none());
    assert!(json.is_some());
}

#[tokio::test]
async fn batch_get_objects() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut client: LedgerServiceClient<Channel> =
        LedgerServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    let BatchGetObjectsResponse { objects, .. } = client
        .batch_get_objects({
            let mut message = BatchGetObjectsRequest::default();
            message.requests = vec![
                GetObjectRequest::new(&Address::from_hex("0x1").unwrap()),
                GetObjectRequest::new(&Address::from_hex("0x2").unwrap()),
                GetObjectRequest::new(&Address::from_hex("0x3").unwrap()),
            ];
            message
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(
        objects[0].object().object_id,
        Some("0x1".parse::<Address>().unwrap().to_string()),
    );
    assert_eq!(
        objects[1].object().object_id,
        Some("0x2".parse::<Address>().unwrap().to_string()),
    );
    assert_eq!(
        objects[2].object().object_id,
        Some("0x3".parse::<Address>().unwrap().to_string()),
    );
}
