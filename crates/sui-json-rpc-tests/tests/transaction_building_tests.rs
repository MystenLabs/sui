// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc::error::SuiRpcInputError;
use sui_json_rpc_api::ReadApiClient;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiTxObjectInfo};
use sui_macros::sim_test;
use sui_types::base_types::ObjectID;
use sui_types::error::UserInputError;
use sui_types::object::{Object, Owner};
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_multi_get_object_info_for_tx_building() {
    let shared_object = Object::shared_for_testing();
    let initial_shared_version = match shared_object.owner {
        Owner::Shared {
            initial_shared_version,
            ..
        } => initial_shared_version,
        _ => panic!("Expected shared object"),
    };
    let owned_object = Object::new_gas_for_testing();
    let immutable_object = Object::immutable_for_testing();
    let cluster = TestClusterBuilder::new()
        .with_objects(
            [
                shared_object.clone(),
                owned_object.clone(),
                immutable_object.clone(),
            ]
            .into_iter(),
        )
        .build()
        .await;
    let http_client = cluster.rpc_client();
    let mut objects = http_client
        .multi_get_objects(
            vec![owned_object.id(), immutable_object.id()],
            Some(SuiObjectDataOptions::new().with_owner()),
        )
        .await
        .unwrap();
    let imm_obj = objects.pop().unwrap().data.unwrap();
    let owned_obj = objects.pop().unwrap().data.unwrap();
    let results = http_client
        .multi_get_object_info_for_tx_building(vec![
            shared_object.id(),
            owned_object.id(),
            immutable_object.id(),
        ])
        .await
        .unwrap();
    assert_eq!(
        results,
        vec![
            SuiTxObjectInfo::Shared {
                initial_shared_version,
            },
            SuiTxObjectInfo::AddressOwned {
                version: owned_obj.version,
                digest: owned_obj.digest,
                owner: owned_obj
                    .owner
                    .unwrap()
                    .get_address_owner_address()
                    .unwrap(),
            },
            SuiTxObjectInfo::Immutable {
                version: imm_obj.version,
                digest: imm_obj.digest,
            }
        ]
    )
}

#[sim_test]
async fn test_multi_get_object_info_for_tx_building_invalid_object() {
    let owned_object = Object::new_gas_for_testing();
    let child_object = Object::with_object_owner_for_testing(ObjectID::random(), owned_object.id());
    let cluster = TestClusterBuilder::new()
        .with_objects([owned_object.clone(), child_object.clone()].into_iter())
        .build()
        .await;
    let http_client = cluster.rpc_client();
    let results = http_client
        .multi_get_object_info_for_tx_building(vec![child_object.id()])
        .await;
    assert!(results.unwrap_err().to_string().contains(
        &UserInputError::InvalidChildObjectArgument {
            child_id: child_object.id(),
            parent_id: owned_object.id(),
        }
        .to_string()
    ));
    let random_id = ObjectID::random();
    let results = http_client
        .multi_get_object_info_for_tx_building(vec![random_id])
        .await;
    assert!(results.unwrap_err().to_string().contains(
        &UserInputError::ObjectNotFound {
            object_id: random_id,
            version: None,
        }
        .to_string()
    ));
}

#[sim_test]
async fn test_multi_get_object_info_for_tx_building_limit() {
    let owned_object = Object::new_gas_for_testing();
    let cluster = TestClusterBuilder::new()
        .with_objects([owned_object.clone()])
        .build()
        .await;
    let http_client = cluster.rpc_client();
    let results = http_client
        .multi_get_object_info_for_tx_building(
            (0..2048).map(|_| owned_object.id()).collect::<Vec<_>>(),
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 2048);
    let results = http_client
        .multi_get_object_info_for_tx_building(
            (0..2049).map(|_| owned_object.id()).collect::<Vec<_>>(),
        )
        .await;
    assert!(results
        .unwrap_err()
        .to_string()
        .contains(&SuiRpcInputError::SizeLimitExceeded(2048.to_string()).to_string()));
}
