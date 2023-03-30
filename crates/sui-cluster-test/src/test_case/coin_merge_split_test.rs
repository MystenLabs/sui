// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{helper::ObjectChecker, TestCaseImpl, TestContext};
use async_trait::async_trait;
use jsonrpsee::rpc_params;
use sui_json_rpc_types::{SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::object::Owner;
use sui_types::sui_serde::BigInt;
use tracing::{debug, info};

pub struct CoinMergeSplitTest;

#[async_trait]
impl TestCaseImpl for CoinMergeSplitTest {
    fn name(&self) -> &'static str {
        "CoinMergeSplit"
    }

    fn description(&self) -> &'static str {
        "Test merge and split SUI coins"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        let mut sui_objs = ctx.get_sui_from_faucet(Some(2)).await;
        let gas_obj = sui_objs.swap_remove(0);

        let signer = ctx.get_wallet_address();
        let primary_coin = sui_objs.swap_remove(0);
        let primary_coin_id = *primary_coin.id();
        let original_value = primary_coin.value();

        // Split
        info!("Testing coin split.");
        let amounts = vec![1.into(), ((original_value - 2) / 2).into()];

        let response =
            Self::split_coin(ctx, signer, *primary_coin.id(), amounts, *gas_obj.id()).await;
        let tx_digest = response.digest;
        let new_coins = response.effects.as_ref().unwrap().created();

        // Verify fullnode observes the txn
        ctx.let_fullnode_sync(vec![tx_digest], 5).await;

        let _ = futures::future::join_all(
            new_coins
                .iter()
                .map(|coin_ref| {
                    ObjectChecker::new(coin_ref.reference.object_id)
                        .owner(Owner::AddressOwner(signer))
                        .check_into_gas_coin(ctx.get_fullnode_client())
                })
                .collect::<Vec<_>>(),
        )
        .await;

        // Merge
        info!("Testing coin merge.");
        let mut coins_merged = Vec::new();
        let mut txes = Vec::new();
        // We on purpose linearize the merge operations, otherwise the primary coin may be locked
        for new_coin in new_coins {
            let coin_to_merge = new_coin.reference.object_id;
            debug!(
                "Merging coin {} back to {}.",
                coin_to_merge, primary_coin_id
            );
            let response =
                Self::merge_coin(ctx, signer, primary_coin_id, coin_to_merge, *gas_obj.id()).await;
            debug!("Verifying the merged coin {} is deleted.", coin_to_merge);
            coins_merged.push(coin_to_merge);
            txes.push(response.digest);
        }

        // Verify fullnode observes the txn
        ctx.let_fullnode_sync(txes, 5).await;

        let _ = futures::future::join_all(
            coins_merged
                .iter()
                .map(|obj_id| {
                    ObjectChecker::new(*obj_id)
                        .owner(Owner::AddressOwner(signer))
                        .deleted()
                        .check(ctx.get_fullnode_client())
                })
                .collect::<Vec<_>>(),
        )
        .await
        .into_iter()
        .collect::<Vec<_>>();

        // Owner still owns the primary coin
        debug!(
            "Verifying owner still owns the primary coin {}",
            *primary_coin.id()
        );
        let primary_after_merge = ObjectChecker::new(primary_coin_id)
            .owner(Owner::AddressOwner(ctx.get_wallet_address()))
            .check_into_gas_coin(ctx.get_fullnode_client())
            .await;
        assert_eq!(
            primary_after_merge.value(),
            original_value,
            "Split-then-merge yields unexpected coin value, expect {}, got {}",
            original_value,
            primary_after_merge.value(),
        );
        Ok(())
    }
}

impl CoinMergeSplitTest {
    async fn merge_coin(
        ctx: &TestContext,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas_obj_id: ObjectID,
    ) -> SuiTransactionBlockResponse {
        let gas_price = ctx.get_reference_gas_price().await;
        let params = rpc_params![
            signer,
            primary_coin,
            coin_to_merge,
            Some(gas_obj_id),
            (2_000_000 * gas_price).to_string()
        ];

        let data = ctx
            .build_transaction_remotely("unsafe_mergeCoins", params)
            .await
            .unwrap();

        ctx.sign_and_execute(data, "coin merge").await
    }

    async fn split_coin(
        ctx: &TestContext,
        signer: SuiAddress,
        primary_coin: ObjectID,
        amounts: Vec<BigInt<u64>>,
        gas_obj_id: ObjectID,
    ) -> SuiTransactionBlockResponse {
        let gas_price = ctx.get_reference_gas_price().await;
        let params = rpc_params![
            signer,
            primary_coin,
            amounts,
            Some(gas_obj_id),
            (2_000_000 * gas_price).to_string()
        ];

        let data = ctx
            .build_transaction_remotely("unsafe_splitCoin", params)
            .await
            .unwrap();

        ctx.sign_and_execute(data, "coin merge").await
    }
}
