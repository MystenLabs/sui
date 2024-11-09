// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::CoinMetadataCache;
use anyhow::anyhow;
use rand::prelude::IteratorRandom;
use rand::rngs::OsRng;
use shared_crypto::intent::Intent;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use sui_json_rpc_types::{
    ObjectChange, SuiObjectDataOptions, SuiObjectResponseQuery, SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_move_build::BuildConfig;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::transaction::{
    InputObjectKind, Transaction, TransactionData, TransactionDataAPI, TransactionKind,
    TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE,
};
use test_cluster::TestClusterBuilder;

#[tokio::test]
async fn test_cache() {
    let network = TestClusterBuilder::new().build().await;
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;
    let rgp = network.get_reference_gas_price().await;

    // Test publish
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
    let gas = vec![get_random_sui(&client, sender, input_objects).await];
    let data = TransactionData::new_with_gas_coins(
        TransactionKind::programmable(pt.clone()),
        sender,
        gas,
        rgp * TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE,
        rgp,
    );

    let signature = keystore
        .sign_secure(&data.sender(), &data, Intent::sui_transaction())
        .unwrap();
    let response = client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(data.clone(), vec![signature]),
            SuiTransactionBlockResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .map_err(|e| anyhow!("TX execution failed for {data:#?}, error : {e}"))
        .unwrap();
    let object_changes = response.object_changes.unwrap();
    let my_coin_type = object_changes
        .into_iter()
        .find_map(|change| {
            if let ObjectChange::Created { object_type, .. } = change {
                if object_type.to_string().contains("2::coin::TreasuryCap") {
                    let coin_tag = object_type.type_params.into_iter().next().unwrap();
                    return Some(coin_tag);
                }
            }
            None
        })
        .unwrap();

    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(1).unwrap());

    assert_eq!(0, coin_cache.metadata.lock().await.len());

    let _ = coin_cache.get_currency(&GAS::type_tag()).await;

    assert_eq!(1, coin_cache.metadata.lock().await.len());
    assert!(coin_cache.metadata.lock().await.contains(&GAS::type_tag()));
    assert!(!coin_cache.metadata.lock().await.contains(&my_coin_type));

    let _ = coin_cache.get_currency(&my_coin_type).await;

    assert_eq!(1, coin_cache.metadata.lock().await.len());
    assert!(coin_cache.metadata.lock().await.contains(&my_coin_type));
    assert!(!coin_cache.metadata.lock().await.contains(&GAS::type_tag()));
}

async fn get_random_sui(
    client: &SuiClient,
    sender: SuiAddress,
    except: Vec<ObjectID>,
) -> ObjectRef {
    let coins = client
        .read_api()
        .get_owned_objects(
            sender,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            /* cursor */ None,
            /* limit */ None,
        )
        .await
        .unwrap()
        .data;

    let coin_resp = coins
        .iter()
        .filter(|object| {
            let obj = object.object().unwrap();
            obj.is_gas_coin() && !except.contains(&obj.object_id)
        })
        .choose(&mut OsRng)
        .unwrap();

    let coin = coin_resp.object().unwrap();
    (coin.object_id, coin.version, coin.digest)
}
