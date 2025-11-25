// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod test_utils;

use anyhow::anyhow;
use shared_crypto::intent::Intent;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::str::FromStr;
use sui_keys::keystore::AccountKeystore;
use sui_move_build::BuildConfig;
use sui_rosetta::CoinMetadataCache;
use sui_rpc::client::Client as GrpcClient;
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    InputObjectKind, TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE, Transaction,
    TransactionData, TransactionDataAPI, TransactionKind,
};
use test_cluster::TestClusterBuilder;
use test_utils::{execute_transaction, get_random_sui};

#[tokio::test]
async fn test_cache() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let addresses = network.get_addresses();
    let sender = addresses[0];
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["..", "..", "examples", "move", "coin"]);
    let compiled_package = BuildConfig::new_for_testing().build(&path).unwrap();
    let compiled_modules_bytes =
        compiled_package.get_package_bytes(/* with_unpublished_deps */ false);
    let dependencies = compiled_package.get_dependency_storage_package_ids();

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.publish_immutable(compiled_modules_bytes, dependencies);
        builder.finish()
    };
    let input_objects = pt
        .input_objects()
        .unwrap_or_default()
        .iter()
        .flat_map(|obj| {
            if let InputObjectKind::ImmOrOwnedMoveObject((id, ..)) = obj {
                Some(*id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let gas = vec![get_random_sui(&mut client, sender, input_objects).await];
    let data = TransactionData::new_with_gas_coins(
        TransactionKind::programmable(pt.clone()),
        sender,
        gas,
        rgp * TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE,
        rgp,
    );

    let signature = keystore
        .sign_secure(&data.sender(), &data, Intent::sui_transaction())
        .await
        .unwrap();
    let signed_tx = Transaction::from_data(data.clone(), vec![signature]);
    let response = execute_transaction(&mut client, &signed_tx)
        .await
        .map_err(|e| anyhow!("TX execution failed for {data:#?}, error : {e}"))
        .unwrap();

    // Extract specifically the MY_COIN type (not MY_COIN_NEW or others)
    // MY_COIN uses the old coin::create_currency API which always creates metadata
    let my_coin_type = response
        .objects_opt()
        .and_then(|object_set| {
            object_set.objects.iter().find_map(|obj| {
                obj.object_type_opt().and_then(|otype| {
                    // Look specifically for MY_COIN, not MY_COIN_NEW or other variants
                    if otype.contains("::coin::TreasuryCap<")
                        && otype.contains("::my_coin::MY_COIN>")
                    {
                        let start = otype.find('<')?;
                        let end = otype.rfind('>')?;
                        let type_str = &otype[start + 1..end];
                        Some(sui_types::TypeTag::from_str(type_str).unwrap())
                    } else {
                        None
                    }
                })
            })
        })
        .expect("MY_COIN treasury cap not found");

    let client = GrpcClient::new(network.rpc_url()).unwrap();
    let coin_cache = CoinMetadataCache::new(client, NonZeroUsize::new(1).unwrap());

    assert_eq!(0, coin_cache.len().await);

    let _ = coin_cache.get_currency(&GAS::type_tag()).await;

    assert_eq!(1, coin_cache.len().await);
    assert!(coin_cache.contains(&GAS::type_tag()).await);
    assert!(!coin_cache.contains(&my_coin_type).await);

    let _ = coin_cache.get_currency(&my_coin_type).await;

    assert_eq!(1, coin_cache.len().await);
    assert!(coin_cache.contains(&my_coin_type).await);
    assert!(!coin_cache.contains(&GAS::type_tag()).await);
}
