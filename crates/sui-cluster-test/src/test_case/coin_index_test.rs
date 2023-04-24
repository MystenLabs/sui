// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext};
use async_trait::async_trait;
use jsonrpsee::rpc_params;
use move_core_types::language_storage::StructTag;
use serde_json::json;
use std::collections::HashMap;
use sui_core::test_utils::compile_managed_coin_package;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::ObjectChange;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_json_rpc_types::{Balance, SuiTransactionBlockResponseOptions};
use sui_types::base_types::{ObjectID, ObjectRef};
use sui_types::gas_coin::GAS;
use sui_types::messages::ExecuteTransactionRequestType;
use sui_types::object::Owner;
use test_utils::messages::make_staking_transaction_with_wallet_context;
use tracing::info;

pub struct CoinIndexTest;

#[async_trait]
impl TestCaseImpl for CoinIndexTest {
    fn name(&self) -> &'static str {
        "CoinIndex"
    }

    fn description(&self) -> &'static str {
        "Test coin index"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        let account = ctx.get_wallet_address();
        let client = ctx.clone_fullnode_client();
        let rgp = ctx.get_reference_gas_price().await;

        // 0. Get some coins first
        ctx.get_sui_from_faucet(None).await;

        // Record initial balances
        let Balance {
            coin_object_count: mut old_coin_object_count,
            total_balance: mut old_total_balance,
            ..
        } = client.coin_read_api().get_balance(account, None).await?;

        // 1. Execute one transfer coin transaction (to another address)
        let txn = ctx.make_transactions(1).await.swap_remove(0);
        let response = client
            .quorum_driver_api()
            .execute_transaction_block(
                txn,
                SuiTransactionBlockResponseOptions::new()
                    .with_effects()
                    .with_balance_changes(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;

        let balance_change = response.balance_changes.unwrap();
        let owner_balance = balance_change
            .iter()
            .find(|b| b.owner == Owner::AddressOwner(account))
            .unwrap();
        let recipient_balance = balance_change
            .iter()
            .find(|b| b.owner != Owner::AddressOwner(account))
            .unwrap();
        let Balance {
            coin_object_count,
            total_balance,
            coin_type,
            ..
        } = client.coin_read_api().get_balance(account, None).await?;
        assert_eq!(coin_type, GAS::type_().to_string());

        assert_eq!(coin_object_count, old_coin_object_count);
        assert_eq!(
            total_balance,
            (old_total_balance as i128 + owner_balance.amount) as u128
        );
        old_coin_object_count = coin_object_count;
        old_total_balance = total_balance;

        let Balance {
            coin_object_count,
            total_balance,
            ..
        } = client
            .coin_read_api()
            .get_balance(recipient_balance.owner.get_owner_address().unwrap(), None)
            .await?;
        assert_eq!(coin_object_count, 1);
        assert!(recipient_balance.amount > 0);
        assert_eq!(total_balance, recipient_balance.amount as u128);

        // 2. Test Staking
        let validator_addr = ctx
            .get_latest_sui_system_state()
            .await
            .active_validators
            .get(0)
            .unwrap()
            .sui_address;
        let txn =
            make_staking_transaction_with_wallet_context(ctx.get_wallet_mut(), validator_addr)
                .await;

        let response = client
            .quorum_driver_api()
            .execute_transaction_block(
                txn,
                SuiTransactionBlockResponseOptions::new()
                    .with_effects()
                    .with_balance_changes(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;

        let balance_change = &response.balance_changes.unwrap()[0];
        assert_eq!(balance_change.owner, Owner::AddressOwner(account));

        let Balance {
            coin_object_count,
            total_balance,
            ..
        } = client.coin_read_api().get_balance(account, None).await?;
        assert_eq!(coin_object_count, old_coin_object_count - 1); // an object is staked
        assert_eq!(
            total_balance,
            (old_total_balance as i128 + balance_change.amount) as u128,
            "total_balance: {}, old_total_balance: {}, sui_balance_change.amount: {}",
            total_balance,
            old_total_balance,
            balance_change.amount
        );
        old_coin_object_count = coin_object_count;

        // 3. Publish a new token package MANAGED
        let (package, cap, envelope) = publish_managed_coin_package(ctx).await?;
        let Balance { total_balance, .. } =
            client.coin_read_api().get_balance(account, None).await?;
        old_total_balance = total_balance;

        info!(
            "token package published, package: {:?}, cap: {:?}",
            package, cap
        );
        let sui_type_str = "0x2::sui::SUI";
        let coin_type_str = format!("{}::managed::MANAGED", package.0);
        info!("coin type: {}", coin_type_str);

        // 4. Mint 1 MANAGED coin to account, balance 10000
        let args = vec![
            SuiJsonValue::from_object_id(cap.0),
            SuiJsonValue::new(json!("10000"))?,
            SuiJsonValue::new(json!(account))?,
        ];
        let txn = client
            .transaction_builder()
            .move_call(
                account,
                package.0,
                "managed",
                "mint",
                vec![],
                args,
                None,
                rgp * 2_000_000,
            )
            .await
            .unwrap();
        let response = ctx.sign_and_execute(txn, "mint managed coin to self").await;

        let balance_changes = &response.balance_changes.unwrap();
        let sui_balance_change = balance_changes
            .iter()
            .find(|b| b.coin_type.to_string().contains("SUI"))
            .unwrap();
        let managed_balance_change = balance_changes
            .iter()
            .find(|b| b.coin_type.to_string().contains("MANAGED"))
            .unwrap();

        assert_eq!(sui_balance_change.owner, Owner::AddressOwner(account));
        assert_eq!(managed_balance_change.owner, Owner::AddressOwner(account));

        let Balance { total_balance, .. } =
            client.coin_read_api().get_balance(account, None).await?;
        assert_eq!(coin_object_count, old_coin_object_count);
        assert_eq!(
            total_balance,
            (old_total_balance as i128 + sui_balance_change.amount) as u128,
            "total_balance: {}, old_total_balance: {}, sui_balance_change.amount: {}",
            total_balance,
            old_total_balance,
            sui_balance_change.amount
        );
        old_coin_object_count = coin_object_count;

        let Balance {
            coin_object_count: managed_coin_object_count,
            total_balance: managed_total_balance,
            // Important: update coin_type_str here because the leading 0s are truncated!
            coin_type: coin_type_str,
            ..
        } = client
            .coin_read_api()
            .get_balance(account, Some(coin_type_str.clone()))
            .await?;
        assert_eq!(managed_coin_object_count, 1); // minted one object
        assert_eq!(
            managed_total_balance,
            10000, // mint amount
        );

        let mut balances = client.coin_read_api().get_all_balances(account).await?;
        let mut expected_balances = vec![
            Balance {
                coin_type: sui_type_str.into(),
                coin_object_count: old_coin_object_count,
                total_balance,
                locked_balance: HashMap::new(),
            },
            Balance {
                coin_type: coin_type_str.clone(),
                coin_object_count: 1,
                total_balance: 10000,
                locked_balance: HashMap::new(),
            },
        ];
        // Comes with asc order.
        expected_balances.sort_by(|l: &Balance, r| l.coin_type.cmp(&r.coin_type));
        balances.sort_by(|l: &Balance, r| l.coin_type.cmp(&r.coin_type));

        assert_eq!(balances, expected_balances,);

        // 5. Mint another MANAGED coin to account, balance 10
        let txn = client
            .transaction_builder()
            .move_call(
                account,
                package.0,
                "managed",
                "mint",
                vec![],
                vec![
                    SuiJsonValue::from_object_id(cap.0),
                    SuiJsonValue::new(json!("10"))?,
                    SuiJsonValue::new(json!(account))?,
                ],
                None,
                rgp * 2_000_000,
            )
            .await
            .unwrap();
        let response = ctx.sign_and_execute(txn, "mint managed coin to self").await;
        assert!(response.status_ok().unwrap());

        let managed_balance = client
            .coin_read_api()
            .get_balance(account, Some(coin_type_str.clone()))
            .await
            .unwrap();
        let managed_coins = client
            .coin_read_api()
            .get_coins(account, Some(coin_type_str.clone()), None, None)
            .await
            .unwrap()
            .data;
        assert_eq!(managed_balance.total_balance, 10000 + 10);
        assert_eq!(managed_balance.coin_object_count, 1 + 1);
        assert_eq!(managed_coins.len(), 1 + 1);
        let managed_old_total_balance = managed_balance.total_balance;
        let managed_old_total_count = managed_balance.coin_object_count;

        // 6. Put the balance 10 MANAGED coin into the envelope
        let managed_coin_id = managed_coins
            .iter()
            .find(|c| c.balance == 10)
            .unwrap()
            .coin_object_id;
        let managed_coin_id_10k = managed_coins
            .iter()
            .find(|c| c.balance == 10000)
            .unwrap()
            .coin_object_id;
        let _ = add_to_envelope(ctx, package.0, envelope.0, managed_coin_id).await;

        let managed_balance = client
            .coin_read_api()
            .get_balance(account, Some(coin_type_str.clone()))
            .await
            .unwrap();
        assert_eq!(
            managed_balance.total_balance,
            managed_old_total_balance - 10
        );
        assert_eq!(
            managed_balance.coin_object_count,
            managed_old_total_count - 1
        );
        let managed_old_total_balance = managed_balance.total_balance;
        let managed_old_total_count = managed_balance.coin_object_count;

        // 7. take back the balance 10 MANAGED coin
        let args = vec![SuiJsonValue::from_object_id(envelope.0)];
        let txn = client
            .transaction_builder()
            .move_call(
                account,
                package.0,
                "managed",
                "take_from_envelope",
                vec![],
                args,
                None,
                rgp * 2_000_000,
            )
            .await
            .unwrap();
        let response = ctx
            .sign_and_execute(txn, "take back managed coin from envelope")
            .await;
        assert!(response.status_ok().unwrap());
        let managed_balance = client
            .coin_read_api()
            .get_balance(account, Some(coin_type_str.clone()))
            .await
            .unwrap();
        assert_eq!(
            managed_balance.total_balance,
            managed_old_total_balance + 10
        );
        assert_eq!(
            managed_balance.coin_object_count,
            managed_old_total_count + 1
        );

        // 8. Put the balance = 10 MANAGED coin back to envelope
        let _ = add_to_envelope(ctx, package.0, envelope.0, managed_coin_id).await;

        // 9. Take from envelope and burn
        let txn = client
            .transaction_builder()
            .move_call(
                account,
                package.0,
                "managed",
                "take_from_envelope_and_burn",
                vec![],
                vec![
                    SuiJsonValue::from_object_id(cap.0),
                    SuiJsonValue::from_object_id(envelope.0),
                ],
                None,
                rgp * 2_000_000,
            )
            .await
            .unwrap();
        let response = ctx
            .sign_and_execute(txn, "take back managed coin from envelope and burn")
            .await;
        assert!(response.status_ok().unwrap());
        let managed_balance = client
            .coin_read_api()
            .get_balance(account, Some(coin_type_str.clone()))
            .await
            .unwrap();
        // Values are the same as in the end of step 6
        assert_eq!(managed_balance.total_balance, managed_old_total_balance);
        assert_eq!(managed_balance.coin_object_count, managed_old_total_count);

        // 10. Burn the balance=10000 MANAGED coin
        let txn = client
            .transaction_builder()
            .move_call(
                account,
                package.0,
                "managed",
                "burn",
                vec![],
                vec![
                    SuiJsonValue::from_object_id(cap.0),
                    SuiJsonValue::from_object_id(managed_coin_id_10k),
                ],
                None,
                rgp * 2_000_000,
            )
            .await
            .unwrap();
        let response = ctx.sign_and_execute(txn, "burn coin").await;
        assert!(response.status_ok().unwrap());
        let managed_balance = client
            .coin_read_api()
            .get_balance(account, Some(coin_type_str.clone()))
            .await
            .unwrap();
        assert_eq!(managed_balance.total_balance, 0);
        assert_eq!(managed_balance.coin_object_count, 0);

        // =========================== Test Get Coins Starts ===========================

        let sui_coins = client
            .coin_read_api()
            .get_coins(account, Some(sui_type_str.into()), None, None)
            .await
            .unwrap()
            .data;

        assert_eq!(
            sui_coins,
            client
                .coin_read_api()
                .get_coins(account, None, None, None)
                .await
                .unwrap()
                .data,
        );
        assert_eq!(
            // this is only SUI coins at the moment
            sui_coins,
            client
                .coin_read_api()
                .get_all_coins(account, None, None)
                .await
                .unwrap()
                .data,
        );

        let sui_balance = client
            .coin_read_api()
            .get_balance(account, None)
            .await
            .unwrap();
        assert_eq!(
            sui_balance.total_balance,
            sui_coins.iter().map(|c| c.balance as u128).sum::<u128>()
        );

        // 11. Mint 40 MANAGED coins with balance 5
        let txn = client
            .transaction_builder()
            .move_call(
                account,
                package.0,
                "managed",
                "mint_multi",
                vec![],
                vec![
                    SuiJsonValue::from_object_id(cap.0),
                    SuiJsonValue::new(json!("5"))?,  // balance = 5
                    SuiJsonValue::new(json!("40"))?, // num = 40
                    SuiJsonValue::new(json!(account))?,
                ],
                None,
                rgp * 2_000_000,
            )
            .await
            .unwrap();
        let response = ctx.sign_and_execute(txn, "multi mint").await;
        assert!(response.status_ok().unwrap());

        let sui_coins = client
            .coin_read_api()
            .get_coins(account, Some(sui_type_str.into()), None, None)
            .await
            .unwrap()
            .data;

        // No more even if ask for more
        assert_eq!(
            sui_coins,
            client
                .coin_read_api()
                .get_coins(account, None, None, Some(sui_coins.len() + 1))
                .await
                .unwrap()
                .data,
        );

        let managed_coins = client
            .coin_read_api()
            .get_coins(account, Some(coin_type_str.clone()), None, None)
            .await
            .unwrap()
            .data;
        let first_managed_coin = managed_coins.first().unwrap().coin_object_id;
        let last_managed_coin = managed_coins.last().unwrap().coin_object_id;

        assert_eq!(managed_coins.len(), 40);
        assert!(managed_coins.iter().all(|c| c.balance == 5));

        let mut total_coins = 0;
        let mut cursor = None;
        loop {
            let page = client
                .coin_read_api()
                .get_all_coins(account, cursor, None)
                .await
                .unwrap();
            total_coins += page.data.len();
            cursor = page.next_cursor;
            if !page.has_next_page {
                break;
            }
        }

        assert_eq!(sui_coins.len() + managed_coins.len(), total_coins,);

        let sui_coins_with_managed_coin_1 = client
            .coin_read_api()
            .get_all_coins(account, None, Some(sui_coins.len() + 1))
            .await
            .unwrap();
        assert_eq!(
            sui_coins_with_managed_coin_1.data.len(),
            sui_coins.len() + 1
        );
        assert_eq!(
            sui_coins_with_managed_coin_1.next_cursor,
            Some(first_managed_coin)
        );
        assert!(sui_coins_with_managed_coin_1.has_next_page);
        let cursor = sui_coins_with_managed_coin_1.next_cursor;

        let managed_coins_2_11 = client
            .coin_read_api()
            .get_all_coins(account, cursor, Some(10))
            .await
            .unwrap();
        assert_eq!(
            managed_coins_2_11,
            client
                .coin_read_api()
                .get_coins(account, Some(coin_type_str.clone()), cursor, Some(10))
                .await
                .unwrap(),
        );

        assert_eq!(managed_coins_2_11.data.len(), 10);
        assert_ne!(
            managed_coins_2_11.data.first().unwrap().coin_object_id,
            first_managed_coin
        );
        assert!(managed_coins_2_11.has_next_page);
        let cursor = managed_coins_2_11.next_cursor;

        let managed_coins_12_40 = client
            .coin_read_api()
            .get_all_coins(account, cursor, None)
            .await
            .unwrap();
        assert_eq!(
            managed_coins_12_40,
            client
                .coin_read_api()
                .get_coins(account, Some(coin_type_str.clone()), cursor, None)
                .await
                .unwrap(),
        );
        assert_eq!(managed_coins_12_40.data.len(), 29);
        assert_eq!(
            managed_coins_12_40.data.last().unwrap().coin_object_id,
            last_managed_coin
        );
        assert!(!managed_coins_12_40.has_next_page);

        let managed_coins_12_40 = client
            .coin_read_api()
            .get_all_coins(account, cursor, Some(30))
            .await
            .unwrap();
        assert_eq!(
            managed_coins_12_40,
            client
                .coin_read_api()
                .get_coins(account, Some(coin_type_str.clone()), cursor, Some(30))
                .await
                .unwrap(),
        );
        assert_eq!(managed_coins_12_40.data.len(), 29);
        assert_eq!(
            managed_coins_12_40.data.last().unwrap().coin_object_id,
            last_managed_coin
        );
        assert!(!managed_coins_12_40.has_next_page);

        // 12. add one coin to envelope, now we only have 39 coins
        let removed_coin_id = managed_coins.get(20).unwrap().coin_object_id;
        let _ = add_to_envelope(ctx, package.0, envelope.0, removed_coin_id).await;
        let managed_coins_12_39 = client
            .coin_read_api()
            .get_all_coins(account, cursor, Some(40))
            .await
            .unwrap();
        assert_eq!(
            managed_coins_12_39,
            client
                .coin_read_api()
                .get_coins(account, Some(coin_type_str.clone()), cursor, Some(40))
                .await
                .unwrap(),
        );
        assert_eq!(managed_coins_12_39.data.len(), 28);
        assert_eq!(
            managed_coins_12_39.data.last().unwrap().coin_object_id,
            last_managed_coin
        );
        assert!(!managed_coins_12_39
            .data
            .iter()
            .any(|coin| coin.coin_object_id == removed_coin_id));
        assert!(!managed_coins_12_39.has_next_page);

        // =========================== Test Get Coins Ends ===========================

        Ok(())
    }
}

async fn publish_managed_coin_package(
    ctx: &mut TestContext,
) -> Result<(ObjectRef, ObjectRef, ObjectRef), anyhow::Error> {
    let compiled_package = compile_managed_coin_package();
    let all_module_bytes =
        compiled_package.get_package_base64(/* with_unpublished_deps */ false);
    let dependencies = compiled_package.get_dependency_original_package_ids();

    let params = rpc_params![
        ctx.get_wallet_address(),
        all_module_bytes,
        dependencies,
        None::<ObjectID>,
        // Doesn't need to be scaled by RGP since most of the cost is storage
        500_000_000.to_string()
    ];

    let data = ctx
        .build_transaction_remotely("unsafe_publish", params)
        .await?;
    let response = ctx.sign_and_execute(data, "publish ft package").await;
    let changes = response.object_changes.unwrap();
    info!("changes: {:?}", changes);
    let pkg = changes
        .iter()
        .find(|change| matches!(change, ObjectChange::Published { .. }))
        .unwrap()
        .object_ref();
    let treasury_cap = changes
        .iter()
        .find(|change| {
            matches!(change, ObjectChange::Created {
            owner: Owner::AddressOwner(_),
            object_type: StructTag {
                name,
                ..
            },
            ..
        } if name.as_str() == "TreasuryCap")
        })
        .unwrap()
        .object_ref();
    let envelope = changes
        .iter()
        .find(|change| {
            matches!(change, ObjectChange::Created {
            owner: Owner::Shared {..},
            object_type: StructTag {
                name,
                ..
            },
            ..
        } if name.as_str() == "PublicRedEnvelope")
        })
        .unwrap()
        .object_ref();
    Ok((pkg, treasury_cap, envelope))
}

async fn add_to_envelope(
    ctx: &mut TestContext,
    pkg_id: ObjectID,
    envelope: ObjectID,
    coin: ObjectID,
) -> SuiTransactionBlockResponse {
    let account = ctx.get_wallet_address();
    let client = ctx.clone_fullnode_client();
    let rgp = ctx.get_reference_gas_price().await;
    let txn = client
        .transaction_builder()
        .move_call(
            account,
            pkg_id,
            "managed",
            "add_to_envelope",
            vec![],
            vec![
                SuiJsonValue::from_object_id(envelope),
                SuiJsonValue::from_object_id(coin),
            ],
            None,
            rgp * 2_000_000,
        )
        .await
        .unwrap();
    let response = ctx
        .sign_and_execute(txn, "add managed coin to envelope")
        .await;
    assert!(response.status_ok().unwrap());
    response
}
