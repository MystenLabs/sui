// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_macros::sim_test;
use sui_move_build::BuildConfig;
use sui_rpc_api::proto::rpc::v2alpha::{
    move_package_service_client::MovePackageServiceClient, GetPackageRequest,
};
use sui_types::{
    base_types::ObjectID,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{TransactionData, TransactionKind},
};
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_get_package_sui_system() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut client = MovePackageServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = ObjectID::from_hex_literal("0x3").unwrap();
    let request = GetPackageRequest {
        package_id: Some(package_id.to_hex_literal()),
    };

    let response = client.get_package(request).await.unwrap().into_inner();

    let package = response.package.expect("Package should exist");

    assert_eq!(
        package.storage_id,
        Some("0x0000000000000000000000000000000000000000000000000000000000000003".to_string())
    );
    assert_eq!(
        package.original_id,
        Some("0x0000000000000000000000000000000000000000000000000000000000000003".to_string())
    );
    assert_eq!(package.version, Some(1));

    let module_names: Vec<_> = package
        .modules
        .iter()
        .filter_map(|m| m.name.as_ref())
        .cloned()
        .collect();

    let expected_modules = vec![
        "genesis",
        "stake_subsidy",
        "staking_pool",
        "storage_fund",
        "sui_system",
        "sui_system_state_inner",
        "validator",
        "validator_cap",
        "validator_set",
        "validator_wrapper",
        "voting_power",
    ];

    assert_eq!(
        module_names,
        expected_modules
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>(),
        "Module list mismatch"
    );
}

#[sim_test]
async fn test_get_package_published() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut client = MovePackageServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let address = test_cluster.get_address_0();

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "trusted_coin"]);
    let compiled_package = BuildConfig::new_for_testing().build(&path).unwrap();
    let compiled_modules_bytes = compiled_package.get_package_bytes(false);
    let dependencies = compiled_package.get_dependency_storage_package_ids();

    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();
    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.publish_immutable(compiled_modules_bytes, dependencies);
    let ptb = builder.finish();
    let gas_data = sui_types::transaction::GasData {
        payment: vec![(gas_object.0, gas_object.1, gas_object.2)],
        owner: address,
        price: gas_price,
        budget: 100_000_000,
    };

    let kind = TransactionKind::ProgrammableTransaction(ptb);
    let tx_data = TransactionData::new_with_gas_data(kind, address, gas_data);
    let txn = test_cluster.wallet.sign_transaction(&tx_data);

    let mut channel = tonic::transport::Channel::from_shared(test_cluster.rpc_url().to_owned())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let transaction = crate::execute_transaction(&mut channel, &txn).await;

    // Extract package ID from changed objects
    let package_id = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            use sui_rpc_api::proto::rpc::v2beta::changed_object::OutputObjectState;
            if o.output_state == Some(OutputObjectState::PackageWrite as i32) {
                o.object_id.clone()
            } else {
                None
            }
        })
        .unwrap();

    let request = GetPackageRequest {
        package_id: Some(package_id.clone()),
    };

    let response = client.get_package(request).await.unwrap().into_inner();
    let package = response.package.expect("Published package should exist");

    assert_eq!(package.storage_id, Some(package_id.clone()));
    assert_eq!(package.original_id, Some(package_id));
    assert_eq!(package.version, Some(1), "New package should be version 1");

    assert_eq!(
        package.modules.len(),
        1,
        "Trusted coin package should have 1 module"
    );

    let module = &package.modules[0];
    assert_eq!(module.name, Some("trusted_coin".to_string()));

    assert!(
        module.data_types.is_empty(),
        "GetPackage should not include data types"
    );
    assert!(
        module.functions.is_empty(),
        "GetPackage should not include functions"
    );
}

#[sim_test]
async fn test_get_package_not_found() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut client = MovePackageServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Use a non-existent package ID
    let fake_id = ObjectID::random();
    let request = GetPackageRequest {
        package_id: Some(fake_id.to_hex_literal()),
    };

    let result = client.get_package(request).await;

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
}

#[sim_test]
async fn test_get_package_invalid_object_type() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut client = MovePackageServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Get a coin object (not a package)
    let address = test_cluster.get_address_0();
    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();
    let coin_id = gas_object.0;

    let request = GetPackageRequest {
        package_id: Some(coin_id.to_hex_literal()),
    };

    let result = client.get_package(request).await;

    // Should return INVALID_ARGUMENT error
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
}

#[sim_test]
async fn test_get_package_invalid_id() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut client = MovePackageServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetPackageRequest { package_id: None };
    let result = client.get_package(request).await;

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
}

#[sim_test]
async fn test_get_package_invalid_hex() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut client = MovePackageServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetPackageRequest {
        package_id: Some("invalid-hex-string".to_string()), // Invalid hex string
    };
    let result = client.get_package(request).await;

    // Should return INVALID_ARGUMENT error
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
}
