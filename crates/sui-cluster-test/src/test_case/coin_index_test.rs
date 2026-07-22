// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext};
use async_trait::async_trait;
use move_core_types::language_storage::{StructTag, TypeTag};
use serde_json::json;
use sui_json::SuiJsonValue;
use sui_move_build::test_utils::compile_managed_coin_package;
use sui_rpc_api::client::ExecutedTransaction;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::coin::{COIN_MODULE_NAME, COIN_STRUCT_NAME};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::{GAS, GasCoin};
use sui_types::object::Owner;
use sui_types::{Identifier, SUI_FRAMEWORK_ADDRESS};
use tracing::info;

pub struct CoinIndexTest;

/// A decoded coin object owned by an address: its ID and current balance in
/// MIST (the coin's `Coin::value`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct OwnedCoin {
    id: ObjectID,
    balance: u64,
}

#[async_trait]
impl TestCaseImpl for CoinIndexTest {
    fn name(&self) -> &'static str {
        "CoinIndex"
    }

    fn description(&self) -> &'static str {
        "Test coin index / owned-object enumeration via StateService"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        let account = ctx.get_wallet_address();

        // 0. Get some coins first. The coin used below is funded up front so it
        //    is already counted in the initial snapshot; the transfer splits a
        //    small amount off it (as gas coin) to a fresh recipient, leaving the
        //    account's coin count unchanged.
        let coins = ctx.get_sui_from_faucet(Some(1)).await;
        let gas_coin_id = *coins[0].id();

        // Record initial SUI balance + coin count (StateService).
        let mut old_total_balance = Self::sui_balance(ctx, account).await;
        let mut old_coin_object_count = Self::sui_coins(ctx, account).await.len();

        // 1. Execute one transfer coin transaction (to another address). A small
        //    amount is split off an already-owned coin (also the gas coin) and
        //    sent to the recipient; the account keeps the (now-smaller) coin, so
        //    its coin count is unchanged.
        let recipient = SuiAddress::random_for_testing_only();
        let gas_price = ctx.get_reference_gas_price().await;
        let gas_ref = ctx.current_object_ref(gas_coin_id).await;
        let txn_data = {
            use sui_test_transaction_builder::TestTransactionBuilder;
            TestTransactionBuilder::new(account, gas_ref, gas_price)
                .transfer_sui(Some(1_000_000), recipient)
                .build()
        };
        let response = ctx.sign_and_execute(txn_data, "transfer").await;
        let (owner_change, recipient_change) = Self::split_sui_balance_changes(&response, account);

        let total_balance = Self::sui_balance(ctx, account).await;
        let coin_object_count = Self::sui_coins(ctx, account).await.len();
        assert_eq!(coin_object_count, old_coin_object_count);
        assert_eq!(
            total_balance,
            (old_total_balance as i128 + owner_change.1) as u128
        );
        old_coin_object_count = coin_object_count;
        // `old_total_balance` is refreshed after the publish below (its value
        // here would be overwritten before use).

        // The recipient balance change comes from the (already-checkpointed)
        // effects and is authoritative.
        assert!(recipient_change.1 > 0);
        // Classic `TransferSui` sends the recipient a distinct `Coin<SUI>`
        // object (address-balance accumulator crediting only applies to
        // `send_funds`-style transfers), so the legacy assertion holds: the
        // recipient owns exactly one SUI coin worth the transferred amount.
        // The index may lag the checkpoint by a moment, so settle on the
        // balance first, then enumerate the owned coins.
        let recipient_total = Self::balance_until(ctx, recipient, &GAS::type_(), |b| {
            b == recipient_change.1 as u128
        })
        .await;
        info!(
            "recipient {recipient}: balance {recipient_total}, change {}",
            recipient_change.1
        );
        assert_eq!(recipient_total, recipient_change.1 as u128);
        let recipient_coins = Self::sui_coins(ctx, recipient).await;
        assert_eq!(
            recipient_coins.len(),
            1,
            "classic TransferSui must yield exactly one recipient Coin<SUI>"
        );
        assert_eq!(recipient_coins[0].balance, recipient_change.1 as u64);

        // 3. Publish a new token package MANAGED.
        let (package, cap, envelope) = publish_managed_coin_package(ctx, gas_coin_id).await?;
        old_total_balance = Self::sui_balance(ctx, account).await;

        info!("token package published, package: {package:?}, cap: {cap:?}");
        let managed_type = managed_coin_type(package.0);

        // 4. Mint 1 MANAGED coin to account, balance 10000.
        Self::mint_managed(ctx, package.0, cap.0, 10000, account, gas_coin_id).await;

        let total_balance = Self::sui_balance(ctx, account).await;
        let coin_object_count = Self::sui_coins(ctx, account).await.len();
        assert_eq!(coin_object_count, old_coin_object_count);
        // The mint's gas cost reduces the SUI balance; just check it did not grow.
        assert!(total_balance <= old_total_balance);

        // Balance APIs (`GetBalance`/`ListBalances`) key on the coin's INNER type
        // `pkg::managed::MANAGED`, whereas owned-object enumeration keys on the
        // object type `Coin<MANAGED>`.
        let managed_inner = managed_inner_type(package.0);

        let managed_coins = Self::coins_of_type(ctx, account, &managed_type).await;
        assert_eq!(managed_coins.len(), 1); // minted one object
        assert_eq!(managed_coins[0].balance, 10000);
        assert_eq!(Self::coin_balance(ctx, account, &managed_type).await, 10000);
        // Require the StateService coin-balance index to reflect the freshly
        // published custom coin (verified live: settles at t=0, no lag).
        assert_eq!(
            Self::balance_of_type(ctx, account, &managed_inner).await,
            10000,
            "GetBalance(MANAGED) should equal the minted amount",
        );

        // ListBalances (StateService::ListBalances) must report both SUI and the
        // MANAGED custom coin (each keyed on its inner type).
        let all_balances = Self::all_balances(ctx, account).await;
        assert!(
            all_balances
                .iter()
                .any(|(t, _)| Self::type_matches(t, &GAS::type_())),
            "list_balances should include SUI",
        );
        let managed_reported = all_balances
            .iter()
            .find(|(t, _)| Self::type_matches(t, &managed_inner))
            .map(|(_, b)| *b)
            .expect("list_balances should include the MANAGED custom coin");
        assert_eq!(
            managed_reported, 10000,
            "list_balances MANAGED should equal the minted amount",
        );

        // 5. Mint another MANAGED coin to account, balance 10.
        Self::mint_managed(ctx, package.0, cap.0, 10, account, gas_coin_id).await;

        let managed_coins = Self::coins_of_type(ctx, account, &managed_type).await;
        assert_eq!(
            Self::coin_balance(ctx, account, &managed_type).await,
            10000 + 10
        );
        // StateService GetBalance and ListBalances must also reflect the two
        // owned MANAGED coins.
        assert_eq!(
            Self::balance_of_type(ctx, account, &managed_inner).await,
            10000 + 10,
            "GetBalance(MANAGED) should equal the sum of owned MANAGED coins",
        );
        assert_eq!(
            Self::all_balances(ctx, account)
                .await
                .iter()
                .find(|(t, _)| Self::type_matches(t, &managed_inner))
                .map(|(_, b)| *b),
            Some(10000 + 10),
            "ListBalances(MANAGED) should equal the sum of owned MANAGED coins",
        );
        assert_eq!(managed_coins.len(), 2);
        let managed_coin_id = managed_coins.iter().find(|c| c.balance == 10).unwrap().id;
        let managed_coin_id_10k = managed_coins
            .iter()
            .find(|c| c.balance == 10000)
            .unwrap()
            .id;

        // 6. Put the balance-10 MANAGED coin into the envelope (wrap).
        add_to_envelope(ctx, package.0, envelope.0, managed_coin_id, gas_coin_id).await;
        assert_eq!(Self::coin_balance(ctx, account, &managed_type).await, 10000);
        assert_eq!(
            Self::coins_of_type(ctx, account, &managed_type).await.len(),
            1
        );

        // 7. Take back the balance-10 MANAGED coin (unwrap).
        Self::call_managed(
            ctx,
            package.0,
            "take_from_envelope",
            vec![SuiJsonValue::from_object_id(envelope.0)],
            gas_coin_id,
        )
        .await;
        assert_eq!(
            Self::coin_balance(ctx, account, &managed_type).await,
            10000 + 10
        );
        assert_eq!(
            Self::coins_of_type(ctx, account, &managed_type).await.len(),
            2
        );

        // 8. Put the balance-10 MANAGED coin back into the envelope.
        add_to_envelope(ctx, package.0, envelope.0, managed_coin_id, gas_coin_id).await;

        // 9. Take from envelope and burn.
        Self::call_managed(
            ctx,
            package.0,
            "take_from_envelope_and_burn",
            vec![
                SuiJsonValue::from_object_id(cap.0),
                SuiJsonValue::from_object_id(envelope.0),
            ],
            gas_coin_id,
        )
        .await;
        assert_eq!(Self::coin_balance(ctx, account, &managed_type).await, 10000);
        assert_eq!(
            Self::coins_of_type(ctx, account, &managed_type).await.len(),
            1
        );

        // 10. Burn the balance-10000 MANAGED coin.
        Self::call_managed(
            ctx,
            package.0,
            "burn",
            vec![
                SuiJsonValue::from_object_id(cap.0),
                SuiJsonValue::from_object_id(managed_coin_id_10k),
            ],
            gas_coin_id,
        )
        .await;
        assert_eq!(Self::coin_balance(ctx, account, &managed_type).await, 0);
        // GetBalance must report zero once all MANAGED coins are burned.
        assert_eq!(
            Self::balance_of_type(ctx, account, &managed_inner).await,
            0,
            "GetBalance(MANAGED) should be zero after burning all MANAGED coins",
        );
        // ListBalances must agree: MANAGED is either absent from the listing or
        // reported as zero.
        assert_eq!(
            Self::all_balances(ctx, account)
                .await
                .iter()
                .find(|(t, _)| Self::type_matches(t, &managed_inner))
                .map(|(_, b)| *b)
                .unwrap_or(0),
            0,
            "ListBalances(MANAGED) should be zero/absent after burning all MANAGED coins",
        );
        assert_eq!(
            Self::coins_of_type(ctx, account, &managed_type).await.len(),
            0
        );

        // =========================== All-coins vs SUI-coins ===========================
        // With no MANAGED coins left, the "all coins" enumeration (parameterless
        // `0x2::coin::Coin` filter) must equal the SUI-only enumeration.
        let sui_coins = Self::sui_coins(ctx, account).await;
        let all_coins = Self::all_coins(ctx, account).await;
        assert_eq!(
            sui_coins
                .iter()
                .map(|c| c.id)
                .collect::<std::collections::BTreeSet<_>>(),
            all_coins
                .iter()
                .map(|c| c.id)
                .collect::<std::collections::BTreeSet<_>>(),
            "with only SUI left, all-coins should equal SUI-coins",
        );
        let sui_balance = Self::sui_balance(ctx, account).await;
        assert_eq!(
            sui_balance,
            sui_coins.iter().map(|c| c.balance as u128).sum::<u128>(),
            "SUI balance should equal the sum of SUI coin values",
        );

        // 11. Mint 40 MANAGED coins with balance 5.
        Self::call_managed(
            ctx,
            package.0,
            "mint_multi",
            vec![
                SuiJsonValue::from_object_id(cap.0),
                SuiJsonValue::new(json!("5"))?,  // balance = 5
                SuiJsonValue::new(json!("40"))?, // num = 40
                SuiJsonValue::new(json!(account))?,
            ],
            gas_coin_id,
        )
        .await;

        let managed_coins = Self::coins_of_type(ctx, account, &managed_type).await;
        assert_eq!(managed_coins.len(), 40);
        assert!(managed_coins.iter().all(|c| c.balance == 5));

        // Completeness: all-coins == sui-coins + managed-coins (counts).
        let sui_coins = Self::sui_coins(ctx, account).await;
        let all_coins = Self::all_coins(ctx, account).await;
        assert_eq!(
            sui_coins.len() + managed_coins.len(),
            all_coins.len(),
            "all-coins count should equal SUI + MANAGED counts",
        );

        // Pagination: a page smaller than the full set reports a continuation
        // token, and paging through with opaque tokens visits every coin exactly
        // once.
        let page_size = (sui_coins.len() + 1) as u32;
        let paged = Self::all_coins_paginated(ctx, account, page_size).await;
        assert_eq!(
            paged.len(),
            all_coins.len(),
            "paginated all-coins should visit every coin",
        );
        assert_eq!(
            paged
                .iter()
                .map(|c| c.id)
                .collect::<std::collections::BTreeSet<_>>(),
            all_coins
                .iter()
                .map(|c| c.id)
                .collect::<std::collections::BTreeSet<_>>(),
            "paginated all-coins should match the unpaginated set",
        );

        // 12. Wrap one MANAGED coin into the envelope; it must disappear from the
        //     owned-coin enumeration (excluded because it is now wrapped).
        let removed_coin_id = managed_coins[20].id;
        add_to_envelope(ctx, package.0, envelope.0, removed_coin_id, gas_coin_id).await;
        let managed_after = Self::coins_of_type(ctx, account, &managed_type).await;
        assert_eq!(managed_after.len(), 39);
        assert!(
            !managed_after.iter().any(|c| c.id == removed_coin_id),
            "wrapped coin should be excluded from owned-coin enumeration",
        );
        assert_eq!(
            Self::coin_balance(ctx, account, &managed_type).await,
            39 * 5,
            "balance should exclude the wrapped coin",
        );

        Ok(())
    }
}

impl CoinIndexTest {
    /// Total SUI balance (`StateService::GetBalance`).
    async fn sui_balance(ctx: &TestContext, owner: SuiAddress) -> u128 {
        Self::balance_of_type(ctx, owner, &GAS::type_()).await as u128
    }

    async fn balance_of_type(ctx: &TestContext, owner: SuiAddress, coin_type: &StructTag) -> u64 {
        ctx.get_grpc_client()
            .get_balance(owner, coin_type)
            .await
            .unwrap()
            .balance
            .unwrap_or_default()
    }

    /// Balance of a coin *object* type (e.g. `Coin<MANAGED>`) derived by summing
    /// the BCS-decoded values of owned coin objects (`ListOwnedObjects`). Used to
    /// cross-check the `GetBalance`/`ListBalances` aggregates against the actual
    /// owned objects; both must agree.
    async fn coin_balance(ctx: &TestContext, owner: SuiAddress, coin_type: &StructTag) -> u64 {
        Self::coins_of_type(ctx, owner, coin_type)
            .await
            .iter()
            .map(|c| c.balance)
            .sum()
    }

    /// All balances for an owner (`StateService::ListBalances`), as (type-string, total).
    async fn all_balances(ctx: &TestContext, owner: SuiAddress) -> Vec<(String, u64)> {
        use futures::StreamExt;
        let client = ctx.get_grpc_client();
        let mut stream = Box::pin(client.list_balances(owner));
        let mut out = Vec::new();
        while let Some(balance) = stream.next().await {
            let balance = balance.unwrap();
            out.push((
                balance.coin_type.clone().unwrap_or_default(),
                balance.balance.unwrap_or_default(),
            ));
        }
        out
    }

    /// SUI (`Coin<0x2::sui::SUI>`) coins owned by `owner`.
    ///
    /// NOTE: the owned-object filter is the *object* type `Coin<SUI>`
    /// (`GasCoin::type_()`), not the coin's inner type `0x2::sui::SUI`
    /// (`GAS::type_()`, which `GetBalance`/`ListBalances` use as their `T`).
    async fn sui_coins(ctx: &TestContext, owner: SuiAddress) -> Vec<OwnedCoin> {
        Self::coins_of_type(ctx, owner, &GasCoin::type_()).await
    }

    /// All `Coin<T>` objects owned by `owner`, using the parameterless
    /// `0x2::coin::Coin` filter.
    async fn all_coins(ctx: &TestContext, owner: SuiAddress) -> Vec<OwnedCoin> {
        Self::coins_of_type(ctx, owner, &all_coin_filter()).await
    }

    /// Enumerate all coins of a given type via `StateService::ListOwnedObjects`,
    /// following opaque page tokens to completion.
    async fn coins_of_type(
        ctx: &TestContext,
        owner: SuiAddress,
        coin_type: &StructTag,
    ) -> Vec<OwnedCoin> {
        let client = ctx.get_grpc_client();
        let mut out = Vec::new();
        let mut token = None;
        loop {
            let page = client
                .get_owned_objects(owner, Some(coin_type.clone()), Some(50), token)
                .await
                .unwrap();
            for object in &page.items {
                let balance = sui_types::coin::Coin::extract_balance_if_coin(object)
                    .unwrap()
                    .expect("owned object should be a coin")
                    .1;
                out.push(OwnedCoin {
                    id: object.id(),
                    balance,
                });
            }
            token = page.next_page_token;
            if token.is_none() {
                break;
            }
        }
        out
    }

    /// Read a balance, retrying until `predicate` holds or a bounded number of
    /// attempts is exhausted. Absorbs the brief lag between a transaction being
    /// checkpointed and a freshly-touched owner's balance being reflected in
    /// `StateService`.
    async fn balance_until(
        ctx: &TestContext,
        owner: SuiAddress,
        coin_type: &StructTag,
        predicate: impl Fn(u128) -> bool,
    ) -> u128 {
        for _ in 0..20 {
            let balance = Self::balance_of_type(ctx, owner, coin_type).await as u128;
            if predicate(balance) {
                return balance;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Self::balance_of_type(ctx, owner, coin_type).await as u128
    }

    /// Enumerate all coins using an explicit page size to exercise pagination.
    async fn all_coins_paginated(
        ctx: &TestContext,
        owner: SuiAddress,
        page_size: u32,
    ) -> Vec<OwnedCoin> {
        let client = ctx.get_grpc_client();
        let filter = all_coin_filter();
        let mut out = Vec::new();
        let mut token = None;
        let mut first_page = true;
        loop {
            let page = client
                .get_owned_objects(owner, Some(filter.clone()), Some(page_size), token)
                .await
                .unwrap();
            if first_page {
                assert_eq!(
                    page.items.len() as u32,
                    page_size,
                    "first page should be full",
                );
                assert!(
                    page.next_page_token.is_some(),
                    "a partial enumeration should return a continuation token",
                );
                first_page = false;
            }
            for object in &page.items {
                let balance = sui_types::coin::Coin::extract_balance_if_coin(object)
                    .unwrap()
                    .expect("owned object should be a coin")
                    .1;
                out.push(OwnedCoin {
                    id: object.id(),
                    balance,
                });
            }
            token = page.next_page_token;
            if token.is_none() {
                break;
            }
        }
        out
    }

    /// Split the two SUI balance changes of a simple transfer into
    /// (owner_change, recipient_change) as (address, amount).
    fn split_sui_balance_changes(
        response: &ExecutedTransaction,
        account: SuiAddress,
    ) -> ((SuiAddress, i128), (SuiAddress, i128)) {
        let account_sdk: sui_sdk_types::Address = account.into();
        let owner = response
            .balance_changes
            .iter()
            .find(|b| b.address == account_sdk)
            .expect("owner balance change");
        let recipient = response
            .balance_changes
            .iter()
            .find(|b| b.address != account_sdk)
            .expect("recipient balance change");
        (
            (sdk_addr_to_sui(&owner.address), owner.amount),
            (sdk_addr_to_sui(&recipient.address), recipient.amount),
        )
    }

    fn type_matches(coin_type_str: &str, expected: &StructTag) -> bool {
        sui_types::parse_sui_struct_tag(coin_type_str)
            .map(|t| &t == expected)
            .unwrap_or(false)
    }

    async fn mint_managed(
        ctx: &TestContext,
        pkg: ObjectID,
        cap: ObjectID,
        amount: u64,
        recipient: SuiAddress,
        gas_coin_id: ObjectID,
    ) {
        Self::call_managed(
            ctx,
            pkg,
            "mint",
            vec![
                SuiJsonValue::from_object_id(cap),
                SuiJsonValue::new(json!(amount.to_string())).unwrap(),
                SuiJsonValue::new(json!(recipient)).unwrap(),
            ],
            gas_coin_id,
        )
        .await;
    }

    /// Invoke a `managed` module entry function via the gRPC Move-call builder,
    /// paying with the caller-supplied, already-counted gas coin (its ref is
    /// refreshed each call). Reusing a stable gas coin keeps the account's SUI
    /// coin count and balance accounting well-defined across the test.
    async fn call_managed(
        ctx: &TestContext,
        pkg: ObjectID,
        function: &str,
        args: Vec<SuiJsonValue>,
        gas_coin_id: ObjectID,
    ) -> ExecutedTransaction {
        let account = ctx.get_wallet_address();
        let rgp = ctx.get_reference_gas_price().await;
        let gas_ref = ctx.current_object_ref(gas_coin_id).await;
        let builder = ctx.get_grpc_client().transaction_builder();
        let data = builder
            .move_call(
                account,
                pkg,
                "managed",
                function,
                vec![],
                args,
                Some(gas_ref.0),
                rgp * 2_000_000,
                None,
            )
            .await
            .unwrap();
        let response = ctx.sign_and_execute(data, function).await;
        assert!(response.effects.status().is_ok());
        response
    }
}

fn sdk_addr_to_sui(addr: &sui_sdk_types::Address) -> SuiAddress {
    (*addr).into()
}

/// The parameterless `0x2::coin::Coin` StructTag that matches all `Coin<T>`.
fn all_coin_filter() -> StructTag {
    StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: COIN_MODULE_NAME.to_owned(),
        name: COIN_STRUCT_NAME.to_owned(),
        type_params: vec![],
    }
}

/// `Coin<pkg::managed::MANAGED>`.
/// The object type `Coin<pkg::managed::MANAGED>` (used for owned-object
/// enumeration via `ListOwnedObjects`).
fn managed_coin_type(pkg: ObjectID) -> StructTag {
    sui_types::coin::Coin::type_(TypeTag::Struct(Box::new(managed_inner_type(pkg))))
}

/// The inner coin type `pkg::managed::MANAGED` (the `T` used by
/// `GetBalance`/`ListBalances`, which key on the coin's inner type, not
/// `Coin<T>`).
fn managed_inner_type(pkg: ObjectID) -> StructTag {
    StructTag {
        address: pkg.into(),
        module: Identifier::new("managed").unwrap(),
        name: Identifier::new("MANAGED").unwrap(),
        type_params: vec![],
    }
}

async fn publish_managed_coin_package(
    ctx: &mut TestContext,
    gas_coin_id: ObjectID,
) -> Result<(ObjectRef, ObjectRef, ObjectRef), anyhow::Error> {
    let signer = ctx.get_wallet_address();
    let gas_ref = ctx.current_object_ref(gas_coin_id).await;

    let compiled_package = compile_managed_coin_package().await;
    let compiled_modules =
        compiled_package.get_package_bytes(/* with_unpublished_deps */ false);
    let dependencies = compiled_package.get_dependency_storage_package_ids();

    let builder = ctx.get_grpc_client().transaction_builder();
    let data = builder
        .publish(
            signer,
            compiled_modules,
            dependencies,
            Some(gas_ref.0),
            500_000_000,
        )
        .await?;
    let response = ctx.sign_and_execute(data, "publish ft package").await;

    // Find the published package (from the effects/changed objects), the
    // TreasuryCap and the shared envelope.
    let created = response.effects.created();
    let pkg = response
        .get_new_package_obj()
        .expect("publish should create a package");

    let mut client = ctx.get_grpc_client();
    let mut treasury_cap = None;
    let mut envelope = None;
    for (obj_ref, owner) in &created {
        let object = client.get_object(obj_ref.0).await?;
        let type_name = object.type_().map(|t| t.name().to_string());
        match (type_name.as_deref(), owner) {
            (Some("TreasuryCap"), Owner::AddressOwner(_)) => treasury_cap = Some(*obj_ref),
            (Some("PublicRedEnvelope"), Owner::Shared { .. }) => envelope = Some(*obj_ref),
            _ => {}
        }
    }
    let treasury_cap = treasury_cap.expect("publish should create a TreasuryCap");
    let envelope = envelope.expect("publish should create a shared PublicRedEnvelope");
    info!("published package {pkg:?}, cap {treasury_cap:?}, envelope {envelope:?}");
    Ok((pkg, treasury_cap, envelope))
}

async fn add_to_envelope(
    ctx: &mut TestContext,
    pkg_id: ObjectID,
    envelope: ObjectID,
    coin: ObjectID,
    gas_coin_id: ObjectID,
) -> ExecutedTransaction {
    CoinIndexTest::call_managed(
        ctx,
        pkg_id,
        "add_to_envelope",
        vec![
            SuiJsonValue::from_object_id(envelope),
            SuiJsonValue::from_object_id(coin),
        ],
        gas_coin_id,
    )
    .await
}
