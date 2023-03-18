// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::access::ModuleAccess;
use sui_framework::get_move_stdlib_package;
use sui_json_rpc::api::ReadApiClient;
use sui_json_rpc_types::SuiObjectResponse;
use sui_types::{
    base_types::ObjectID, digests::TransactionDigest, object::Object, SUI_FRAMEWORK_OBJECT_ID,
};
use test_utils::network::TestClusterBuilder;

use sui_macros::sim_test;

#[sim_test]
async fn test_additional_objects() {
    // Test the ability to add additional objects into genesis for test clusters
    let id = ObjectID::random();
    let cluster = TestClusterBuilder::new()
        .with_objects([Object::immutable_with_id_for_testing(id)])
        .build()
        .await
        .unwrap();

    let client = cluster.rpc_client();
    let resp = client.get_object_with_options(id, None).await.unwrap();
    assert!(matches!(resp, SuiObjectResponse::Exists(_)));
}

#[sim_test]
async fn test_package_override() {
    // `with_objects` can be used to override existing packages.
    let id = SUI_FRAMEWORK_OBJECT_ID;

    let framework_ref = {
        let default_cluster = TestClusterBuilder::new().build().await.unwrap();
        let client = default_cluster.rpc_client();
        let SuiObjectResponse::Exists(obj) = client.get_object_with_options(id, None).await.unwrap() else {
            panic!("Original framework package should exist");
        };

        obj.object_ref()
    };

    let modified_ref = {
        let mut framework_modules = sui_framework::get_sui_framework();

        // Create an empty module that is pretending to be part of the sui framework.
        let mut test_module = move_binary_format::file_format::empty_module();
        let address_idx = test_module.self_handle().address.0 as usize;
        test_module.address_identifiers[address_idx] = id.into();

        // Add the dummy module to the rest of the sui-frameworks.  We can't replace the framework
        // entirely because we will call into it for genesis.
        framework_modules.push(test_module);

        let package_override = Object::new_package_for_testing(
            framework_modules,
            TransactionDigest::genesis(),
            &[get_move_stdlib_package()],
        )
        .unwrap();

        let modified_cluster = TestClusterBuilder::new()
            .with_objects([package_override])
            .build()
            .await
            .unwrap();

        let client = modified_cluster.rpc_client();
        let SuiObjectResponse::Exists(obj) = client.get_object_with_options(id, None).await.unwrap() else {
            panic!("Modified framework package should exist");
        };

        obj.object_ref()
    };

    assert_ne!(framework_ref, modified_ref);
}
