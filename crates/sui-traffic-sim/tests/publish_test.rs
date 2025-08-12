// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_move_build::BuildConfig;
use sui_types::transaction::TransactionData;
use test_cluster::TestClusterBuilder;
use tracing::info;

#[sui_macros::sim_test]
async fn test_publish_package() {
    info!("Starting TestCluster...");
    let test_cluster = TestClusterBuilder::new().build().await;
    
    info!("Triggering reconfiguration to epoch 1...");
    test_cluster.trigger_reconfiguration().await;
    test_cluster.wait_for_epoch(Some(1)).await;
    
    // Get a funded account from the test cluster
    let address = test_cluster.get_address_0();
    
    // Get the path to the counter package
    let mut package_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    package_path.push("apps");
    package_path.push("counter");
    
    info!("Publishing counter package from: {:?}", package_path);
    
    // Build the Move package
    let build_config = BuildConfig::new_for_testing();
    let package = build_config.build(&package_path).expect("Failed to build package");
    let compiled_modules = package.get_package_bytes(false);
    let dependencies = package.get_dependency_storage_package_ids();
    
    // Get gas object
    let gas_objects = test_cluster.wallet.get_gas_objects_owned_by_address(address, None).await.unwrap();
    let gas_object = gas_objects.first().unwrap();
    
    // Create publish transaction
    let tx_data = TransactionData::new_module(
        address,
        *gas_object,
        compiled_modules,
        dependencies,
        100_000_000, // gas budget
        test_cluster.get_reference_gas_price().await,
    );
    
    // Execute transaction using wallet
    let tx = test_cluster.wallet.sign_transaction(&tx_data);
    let response = test_cluster.wallet.execute_transaction_may_fail(tx).await.unwrap();
    
    // Extract package ID from effects
    let package_id = response
        .effects
        .unwrap()
        .created()
        .iter()
        .find(|obj| obj.owner.is_immutable())
        .map(|obj| obj.reference.object_id)
        .expect("Failed to extract package ID");
    
    info!("Successfully published package with ID: {}", package_id);
    
    // Verify the package exists by trying to read it
    let _package_object = test_cluster
        .wallet
        .get_client()
        .await
        .unwrap()
        .read_api()
        .get_object_with_options(package_id, Default::default())
        .await
        .expect("Failed to read package object");
    
    info!("Package verification successful");
}