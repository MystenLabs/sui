// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::str::FromStr;

use anyhow::{Result, anyhow};

use shared_crypto::intent::Intent;
use sui_keys::keystore::{AccountKeystore, Keystore};
use sui_move_build::BuildConfig;
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::coin::COIN_MODULE_NAME;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    Command, ObjectArg, Transaction, TransactionData, TransactionDataAPI,
};
use sui_types::{Identifier, SUI_FRAMEWORK_PACKAGE_ID, TypeTag};

const DEFAULT_GAS_BUDGET: u64 = 900_000_000;
pub const TEST_COIN_DECIMALS: u64 = 6;

#[derive(Debug, Clone)]
pub struct InitRet {
    pub owner: SuiAddress,
    pub treasury_cap: ObjectRef,
    pub coin_tag: TypeTag,
    pub changed_objects: Vec<ObjectID>,
}
pub async fn init_package(
    test_cluster: &test_cluster::TestCluster,
    client: &mut GrpcClient,
    keystore: &Keystore,
    sender: SuiAddress,
    path: &Path,
) -> Result<InitRet> {
    let path_buf = path
        .canonicalize()
        .map_err(|e| anyhow!("Failed to canonicalize path {}: {}", path.display(), e))?;

    let move_build_config = BuildConfig::new_for_testing();
    let compiled_modules = move_build_config.build(&path_buf)?;
    let modules_bytes = compiled_modules.get_package_bytes(false);

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.publish_immutable(
            modules_bytes,
            vec![
                ObjectID::from_hex_literal("0x1").unwrap(),
                ObjectID::from_hex_literal("0x2").unwrap(),
            ],
        );
        builder.finish()
    };

    let price = client.get_reference_gas_price().await?;
    let budget = DEFAULT_GAS_BUDGET;
    let (_, gas_object_data) = test_cluster
        .wallet
        .gas_for_owner_budget(sender, budget, Default::default())
        .await?;
    let gas_object = gas_object_data.object_ref();
    let tx_data = TransactionData::new_programmable(sender, vec![gas_object], pt, budget, price);

    let sig = keystore
        .sign_secure(&tx_data.sender(), &tx_data, Intent::sui_transaction())
        .await?;

    let signed_tx = Transaction::from_data(tx_data, vec![sig]);
    let response = crate::test_utils::execute_transaction(client, &signed_tx).await?;

    let effects = response.effects();
    assert!(
        effects.status().success(),
        "Transaction failed: {:?}",
        effects.status().error()
    );

    let mut changed_object_ids = Vec::new();
    for obj in effects.changed_objects() {
        changed_object_ids.push(ObjectID::from_str(obj.object_id())?);
    }

    let treasury_cap = effects
        .changed_objects()
        .iter()
        .find_map(|obj| {
            let type_str = obj.object_type();
            if type_str.contains("TreasuryCap") {
                let object_id = ObjectID::from_str(obj.object_id()).ok()?;
                let version = obj.output_version();
                let digest = obj.output_digest().parse().ok()?;

                let start = type_str.find("TreasuryCap<")?;
                let start_idx = start + "TreasuryCap<".len();
                let end = type_str[start_idx..].find('>')?;
                let coin_type_str = &type_str[start_idx..start_idx + end];
                let coin_tag: TypeTag = coin_type_str.parse().ok()?;

                Some(InitRet {
                    owner: sender,
                    treasury_cap: (object_id, version.into(), digest),
                    coin_tag,
                    changed_objects: changed_object_ids.clone(),
                })
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("No TreasuryCap found in transaction effects"))?;

    Ok(treasury_cap)
}

pub async fn mint(
    test_cluster: &test_cluster::TestCluster,
    client: &mut GrpcClient,
    keystore: &Keystore,
    init_ret: InitRet,
    balances_to: Vec<(u64, SuiAddress)>,
) -> Result<ExecutedTransaction> {
    let treasury_cap_owner = init_ret.owner;
    let price = client.get_reference_gas_price().await?;
    let budget = DEFAULT_GAS_BUDGET;
    let forbidden_objects = init_ret.changed_objects.iter().cloned().collect();
    let (_gas_balance, gas_object_data) = test_cluster
        .wallet
        .gas_for_owner_budget(treasury_cap_owner, budget, forbidden_objects)
        .await?;
    let gas_object = gas_object_data.object_ref();

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

    let tx_data = TransactionData::new_programmable(
        treasury_cap_owner,
        vec![gas_object],
        builder,
        budget,
        price,
    );

    let sig = keystore
        .sign_secure(&tx_data.sender(), &tx_data, Intent::sui_transaction())
        .await?;

    let signed_tx = Transaction::from_data(tx_data, vec![sig]);
    let executed_tx = crate::test_utils::execute_transaction(client, &signed_tx).await?;

    assert!(
        executed_tx.effects().status().success(),
        "Transaction failed: {:?}",
        executed_tx.effects().status().error()
    );

    Ok(executed_tx)
}
