// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use sui_move_build::{BuildConfig, SuiPackageHooks};
use sui_sdk::SuiClient;
use sui_types::messages::{TransactionData, TransactionKind};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use test_utils::network::TestClusterBuilder;

#[tokio::test]
async fn test_dry_run_publish_with_mocked_coin() -> Result<(), anyhow::Error> {
    let mut cluster = TestClusterBuilder::new().build().await.unwrap();
    let context = &mut cluster.wallet;

    let address = &cluster.accounts[0];
    let client: SuiClient = context.get_client().await.unwrap();

    // Publish test coin package
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let compiled_package = BuildConfig::default()
        .build(Path::new("src/unit_tests/data/dummy_modules_publish").to_path_buf())?;
    let compiled_modules_bytes = compiled_package
        .get_package_base64(false)
        .into_iter()
        .map(|b| b.to_vec().unwrap())
        .collect::<Vec<_>>();
    let dependencies = compiled_package.get_dependency_original_package_ids();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.publish_immutable(compiled_modules_bytes, dependencies);

    let publish = TransactionKind::programmable(builder.finish());
    let transaction_bytes =
        TransactionData::new_with_gas_coins(publish, *address, vec![], 100000000, 1000);

    let result = client
        .read_api()
        .dry_run_transaction_block(transaction_bytes)
        .await;

    // Dry run balance change should not fail because of mocked coin
    assert!(result.is_ok());

    Ok(())
}
