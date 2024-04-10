// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_framework::BuiltInFramework;
use sui_json_rpc_api::ReadApiClient;
use sui_json_rpc_types::SuiObjectResponse;
use sui_macros::sim_test;
use sui_types::{
    base_types::ObjectID, digests::TransactionDigest, object::Object, MOVE_STDLIB_PACKAGE_ID,
    SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_ADDRESS, SUI_SYSTEM_PACKAGE_ID,
};
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_additional_objects() {
    // Test the ability to add additional objects into genesis for test clusters
    let id = ObjectID::random();
    let cluster = TestClusterBuilder::new()
        .with_objects([Object::immutable_with_id_for_testing(id)])
        .build()
        .await;

    let client = cluster.rpc_client();
    let resp = client.get_object(id, None).await.unwrap();
    assert!(matches!(resp, SuiObjectResponse { data: Some(_), .. }));
}

#[sim_test]
async fn test_package_override() {
    // `with_objects` can be used to override existing packages.
    let framework_ref = {
        let default_cluster = TestClusterBuilder::new().build().await;
        let client = default_cluster.rpc_client();
        let obj = client
            .get_object(SUI_SYSTEM_PACKAGE_ID, None)
            .await
            .unwrap();

        if let Some(obj) = obj.data {
            obj.object_ref()
        } else {
            panic!("Original framework package should exist");
        }
    };

    let modified_ref = {
        let mut framework_modules = BuiltInFramework::get_package_by_id(&SUI_SYSTEM_PACKAGE_ID)
            .modules()
            .to_vec();

        // Create an empty module that is pretending to be part of the sui framework.
        let mut test_module = move_binary_format::file_format::empty_module();
        let address_idx = test_module.self_handle().address.0 as usize;
        test_module.address_identifiers[address_idx] = SUI_SYSTEM_ADDRESS;

        // Add the dummy module to the rest of the sui-frameworks.  We can't replace the framework
        // entirely because we will call into it for genesis.
        framework_modules.push(test_module);

        let package_override = Object::new_package_for_testing(
            &framework_modules,
            TransactionDigest::genesis_marker(),
            [
                BuiltInFramework::get_package_by_id(&MOVE_STDLIB_PACKAGE_ID).genesis_move_package(),
                BuiltInFramework::get_package_by_id(&SUI_FRAMEWORK_PACKAGE_ID)
                    .genesis_move_package(),
            ],
        )
        .unwrap();

        let modified_cluster = TestClusterBuilder::new()
            .with_objects([package_override])
            .build()
            .await;

        let client = modified_cluster.rpc_client();
        let obj = client
            .get_object(SUI_SYSTEM_PACKAGE_ID, None)
            .await
            .unwrap();

        if let Some(obj) = obj.data {
            obj.object_ref()
        } else {
            panic!("Original framework package should exist");
        }
    };

    assert_ne!(framework_ref, modified_ref);
}
