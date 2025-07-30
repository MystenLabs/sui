// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::str::FromStr;

use anyhow::{anyhow, Result};

use move_cli::base;
use shared_crypto::intent::Intent;
use sui_json_rpc_types::{
    ObjectChange, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::{AccountKeystore, Keystore};
use sui_move_build::BuildConfig;
use sui_rosetta::grpc_client::GrpcClient;

use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::coin::COIN_MODULE_NAME;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::transaction::{
    Command, ObjectArg, Transaction, TransactionData, TransactionDataAPI,
};
use sui_types::{Identifier, TypeTag, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::TestClusterBuilder;
use url::Url;

use tracing::debug;

const DEFAULT_GAS_BUDGET: u64 = 900_000_000;

pub struct GasRet {
    pub object: ObjectRef,
    pub budget: u64,
    pub price: u64,
}

pub async fn select_gas(
    _client: &SuiClient,
    grpc_client: &GrpcClient,
    signer_addr: SuiAddress,
    input_gas: Option<ObjectID>,
    budget: Option<u64>,
    exclude_objects: Vec<ObjectID>,
    gas_price: Option<u64>,
) -> Result<GasRet> {
    let price = match gas_price {
        Some(p) => p,
        None => {
            debug!("No gas price given, fetching from fullnode");
            // Get reference gas price from current epoch
            let epoch = grpc_client.get_epoch(None).await?;
            epoch
                .reference_gas_price
                .ok_or_else(|| anyhow!("No reference gas price in epoch"))?
        }
    };
    let budget = budget.unwrap_or_else(|| {
        debug!("No gas budget given, defaulting to {DEFAULT_GAS_BUDGET}");
        debug_assert!(DEFAULT_GAS_BUDGET > price);
        DEFAULT_GAS_BUDGET
    });
    if budget < price {
        return Err(anyhow!(
            "Gas budget {budget} is less than the reference gas price {price}.
              The gas budget must be at least the current reference gas price of {price}."
        ));
    }

    if let Some(gas) = input_gas {
        // Get object reference using GRPC
        let object_refs = grpc_client.get_object_refs(vec![gas]).await?;
        let object = object_refs
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No object-ref"))?;
        return Ok(GasRet {
            object,
            budget,
            price,
        });
    }

    // Get gas coins using GRPC - use SUI coin type
    let gas_coins = grpc_client
        .get_all_coins(signer_addr, Some("0x2::sui::SUI".to_string()))
        .await?;

    for (object_id, balance, object_ref) in gas_coins {
        if !exclude_objects.contains(&object_id) && balance >= budget {
            return Ok(GasRet {
                object: object_ref,
                budget,
                price,
            });
        }
    }

    Err(anyhow!("Cannot find gas coin for signer address [{signer_addr}] with amount sufficient for the required gas amount [{budget}]."))
}

#[derive(Debug)]
pub struct InitRet {
    pub owner: SuiAddress,
    pub treasury_cap: ObjectRef,
    pub coin_tag: TypeTag,
}
pub async fn init_package(
    client: &SuiClient,
    grpc_client: &GrpcClient,
    keystore: &Keystore,
    sender: SuiAddress,
    path: &Path,
) -> Result<InitRet> {
    let path_buf = base::reroot_path(Some(path))?;

    let move_build_config = BuildConfig::new_for_testing();
    let compiled_modules = move_build_config.build(path_buf.as_path())?;
    let modules_bytes = compiled_modules.get_package_bytes(false);

    let tx_kind = client
        .transaction_builder()
        .publish_tx_kind(
            sender,
            modules_bytes,
            vec![
                ObjectID::from_hex_literal("0x1").unwrap(),
                ObjectID::from_hex_literal("0x2").unwrap(),
            ],
        )
        .await?;

    let gas_data = select_gas(client, grpc_client, sender, None, None, vec![], None).await?;
    let tx_data = TransactionData::new_with_gas_coins_allow_sponsor(
        tx_kind,
        sender,
        vec![gas_data.object],
        gas_data.budget,
        gas_data.price,
        sender,
    );

    let sig = keystore.sign_secure(&tx_data.sender(), &tx_data, Intent::sui_transaction())?;

    let res = client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(tx_data, vec![sig]),
            SuiTransactionBlockResponseOptions::new()
                .with_effects()
                .with_object_changes()
                .with_input(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    let treasury_cap = res.object_changes.unwrap().into_iter().find_map(|change| {
        if let ObjectChange::Created {
            object_type,
            object_id,
            version,
            digest,
            owner,
            ..
        } = change
        {
            if object_type.to_string().contains("2::coin::TreasuryCap") {
                let Owner::AddressOwner(owner) = owner else {
                    return None;
                };
                let coin_tag = object_type.type_params.into_iter().next().unwrap();
                return Some(InitRet {
                    owner,
                    treasury_cap: (object_id, version, digest),
                    coin_tag,
                });
            }
        }
        None
    });

    Ok(treasury_cap.unwrap())
}

pub async fn mint(
    client: &SuiClient,
    grpc_client: &GrpcClient,
    keystore: &Keystore,
    init_ret: InitRet,
    balances_to: Vec<(u64, SuiAddress)>,
) -> Result<SuiTransactionBlockResponse> {
    let treasury_cap_owner = init_ret.owner;
    let gas_data = select_gas(
        client,
        grpc_client,
        treasury_cap_owner,
        None,
        None,
        vec![],
        None,
    )
    .await?;

    let mut ptb = ProgrammableTransactionBuilder::new();

    let treasury_cap = ptb.obj(ObjectArg::ImmOrOwnedObject(init_ret.treasury_cap))?;
    for (balance, to) in balances_to {
        let balance = ptb.pure(balance)?;
        let coin = ptb.command(Command::move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::from(COIN_MODULE_NAME),
            Identifier::from_str("mint")?,
            vec![init_ret.coin_tag.clone()],
            vec![treasury_cap, balance],
        ));
        ptb.transfer_arg(to, coin);
    }
    let builder = ptb.finish();

    // Sign transaction
    let tx_data = TransactionData::new_programmable(
        treasury_cap_owner,
        vec![gas_data.object],
        builder,
        gas_data.budget,
        gas_data.price,
    );

    let sig = keystore.sign_secure(&tx_data.sender(), &tx_data, Intent::sui_transaction())?;

    let res = client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(tx_data, vec![sig]),
            SuiTransactionBlockResponseOptions::new()
                .with_effects()
                .with_object_changes()
                .with_input(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    Ok(res)
}

#[tokio::test]
async fn test_mint() {
    const COIN1_BALANCE: u64 = 100_000_000;
    const COIN2_BALANCE: u64 = 200_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    // Create GRPC client for test
    let grpc_client =
        GrpcClient::new(Url::parse(test_cluster.rpc_url()).unwrap(), None, None).unwrap();

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(
        &client,
        &grpc_client,
        keystore,
        sender,
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/custom_coins/test_coin"
        )),
    )
    .await
    .unwrap();

    let address1 = test_cluster.get_address_1();
    let address2 = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, address1), (COIN2_BALANCE, address2)];

    let mint_res = mint(&client, &grpc_client, keystore, init_ret, balances_to)
        .await
        .unwrap();
    let coins = mint_res
        .object_changes
        .unwrap()
        .into_iter()
        .filter_map(|change| {
            if let ObjectChange::Created {
                object_type, owner, ..
            } = change
            {
                Some((object_type, owner))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let coin1 = coins
        .iter()
        .find(|coin| coin.1.get_address_owner_address().unwrap() == address1)
        .unwrap();
    let coin2 = coins
        .iter()
        .find(|coin| coin.1.get_address_owner_address().unwrap() == address2)
        .unwrap();
    assert!(coin1.0.to_string().contains("::test_coin::TEST_COIN"));
    assert!(coin2.0.to_string().contains("::test_coin::TEST_COIN"));
}
