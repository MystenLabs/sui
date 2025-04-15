// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::field_mask::FieldMask;
use sui_rpc_api::field_mask::FieldMaskUtil;
use sui_rpc_api::proto::rpc::v2beta::ledger_service_client::LedgerServiceClient;
use sui_rpc_api::proto::rpc::v2beta::BatchGetObjectsRequest;
use sui_rpc_api::proto::rpc::v2beta::BatchGetObjectsResponse;
use sui_rpc_api::proto::rpc::v2beta::GetObjectRequest;
use sui_rpc_api::proto::rpc::v2beta::Object;
use sui_sdk_types::ObjectId;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_object() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let id: ObjectId = "0x5".parse().unwrap();

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Request with no provided read_mask
    let Object {
        bcs,
        object_id,
        version,
        digest,
        owner,
        object_type,
        has_public_transfer,
        contents,
        modules,
        type_origin_table,
        linkage_table,
        previous_transaction,
        storage_rebate,
    } = client
        .get_object(GetObjectRequest {
            object_id: Some(id.to_string()),
            version: None,
            read_mask: None,
        })
        .await
        .unwrap()
        .into_inner();

    // These fields default to being read
    assert_eq!(object_id, Some(id.to_string()));
    assert!(version.is_some());
    assert!(digest.is_some());

    // while these fields default to not being read
    assert!(owner.is_none());
    assert!(bcs.is_none());
    assert!(object_type.is_none());
    assert!(has_public_transfer.is_none());
    assert!(contents.is_none());
    assert!(modules.is_empty());
    assert!(type_origin_table.is_empty());
    assert!(linkage_table.is_empty());
    assert!(previous_transaction.is_none());
    assert!(storage_rebate.is_none());

    // Request with provided read_mask
    let Object {
        bcs,
        object_id,
        version,
        digest,
        owner,
        object_type,
        has_public_transfer,
        contents,
        modules,
        type_origin_table,
        linkage_table,
        previous_transaction,
        storage_rebate,
    } = client
        .get_object(GetObjectRequest {
            object_id: Some(id.to_string()),
            version: Some(1),
            read_mask: Some(FieldMask::from_str("object_id,version")),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(object_id, Some(id.to_string()));
    assert_eq!(version, Some(1));

    // These fields were not requested
    assert!(digest.is_none());
    assert!(owner.is_none());
    assert!(bcs.is_none());
    assert!(object_type.is_none());
    assert!(has_public_transfer.is_none());
    assert!(contents.is_none());
    assert!(modules.is_empty());
    assert!(type_origin_table.is_empty());
    assert!(linkage_table.is_empty());
    assert!(previous_transaction.is_none());
    assert!(storage_rebate.is_none());

    let response = client
        .get_object(GetObjectRequest {
            object_id: Some(id.to_string()),
            version: None,
            read_mask: Some(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "bcs",
            ])),
        })
        .await
        .unwrap()
        .into_inner();

    let Object {
        bcs,
        object_id,
        version,
        digest,
        owner,
        object_type,
        has_public_transfer,
        contents,
        modules,
        type_origin_table,
        linkage_table,
        previous_transaction,
        storage_rebate,
    } = &response;

    assert!(object_id.is_some());
    assert!(version.is_some());
    assert!(digest.is_some());
    assert!(bcs.is_some());

    assert!(owner.is_none());
    assert!(object_type.is_none());
    assert!(has_public_transfer.is_none());
    assert!(contents.is_none());
    assert!(modules.is_empty());
    assert!(type_origin_table.is_empty());
    assert!(linkage_table.is_empty());
    assert!(previous_transaction.is_none());
    assert!(storage_rebate.is_none());
}

#[sim_test]
async fn batch_get_objects() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let BatchGetObjectsResponse { objects } = client
        .batch_get_objects(BatchGetObjectsRequest {
            requests: vec![
                GetObjectRequest {
                    object_id: Some("0x1".to_owned()),
                    version: None,
                    read_mask: None,
                },
                GetObjectRequest {
                    object_id: Some("0x2".to_owned()),
                    version: None,
                    read_mask: None,
                },
                GetObjectRequest {
                    object_id: Some("0x3".to_owned()),
                    version: None,
                    read_mask: None,
                },
            ],
            read_mask: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(
        objects[0].object_id,
        Some("0x1".parse::<ObjectId>().unwrap().to_string())
    );
    assert_eq!(
        objects[1].object_id,
        Some("0x2".parse::<ObjectId>().unwrap().to_string())
    );
    assert_eq!(
        objects[2].object_id,
        Some("0x3".parse::<ObjectId>().unwrap().to_string())
    );
}
