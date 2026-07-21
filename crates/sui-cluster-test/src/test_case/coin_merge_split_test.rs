// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext, helper::ObjectChecker};
use async_trait::async_trait;
use sui_rpc_api::client::ExecutedTransaction;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
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
        let mut sui_objs = ctx.get_sui_from_faucet(Some(1)).await;
        let gas_obj = sui_objs.swap_remove(0);
        let gas_obj_id = *gas_obj.id();

        let signer = ctx.get_wallet_address();
        let mut sui_objs_2 = ctx.get_sui_from_faucet(Some(1)).await;

        let primary_coin = sui_objs_2.swap_remove(0);
        let primary_coin_id = *primary_coin.id();
        let original_value = primary_coin.value();

        // Split
        info!("Testing coin split.");
        let amounts = vec![1, (original_value - 2) / 2];

        let response = Self::split_coin(ctx, signer, primary_coin_id, amounts, gas_obj_id).await;
        // The created coins are read straight from the native effects
        // (`LedgerService`).
        let new_coins: Vec<ObjectID> = response
            .effects
            .created()
            .iter()
            .map(|(obj_ref, _owner)| obj_ref.0)
            .collect();
        assert!(
            !new_coins.is_empty(),
            "Expected the split to create new coins"
        );

        for coin_id in &new_coins {
            let _ = ObjectChecker::new(*coin_id)
                .owner(Owner::AddressOwner(signer))
                .check_into_gas_coin(&ctx.get_grpc_client())
                .await;
        }

        // Merge
        info!("Testing coin merge.");
        let mut coins_merged = Vec::new();
        // We on purpose linearize the merge operations, otherwise the primary
        // coin may be locked. Both the primary coin and the gas coin refs are
        // refreshed before every mutation.
        for coin_to_merge in &new_coins {
            debug!(
                "Merging coin {} back to {}.",
                coin_to_merge, primary_coin_id
            );
            let response =
                Self::merge_coin(ctx, signer, primary_coin_id, *coin_to_merge, gas_obj_id).await;
            // The merged-in coin must be reported as deleted by the effects.
            let deleted: Vec<ObjectID> = response.effects.deleted().iter().map(|r| r.0).collect();
            assert!(
                deleted.contains(coin_to_merge),
                "Merged coin {coin_to_merge} should be deleted, deleted set: {deleted:?}",
            );
            coins_merged.push(*coin_to_merge);
        }

        // Owner still owns the primary coin, and its value is restored.
        debug!("Verifying owner still owns the primary coin {primary_coin_id}");
        let primary_after_merge = ObjectChecker::new(primary_coin_id)
            .owner(Owner::AddressOwner(signer))
            .check_into_gas_coin(&ctx.get_grpc_client())
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
    ) -> ExecutedTransaction {
        let gas_ref = ctx.current_object_ref(gas_obj_id).await;
        let builder = ctx.get_grpc_client().transaction_builder();
        let data = builder
            .merge_coins(
                signer,
                primary_coin,
                coin_to_merge,
                Some(gas_ref.0),
                20_000_000,
            )
            .await
            .unwrap();
        ctx.sign_and_execute(data, "coin merge").await
    }

    async fn split_coin(
        ctx: &TestContext,
        signer: SuiAddress,
        primary_coin: ObjectID,
        amounts: Vec<u64>,
        gas_obj_id: ObjectID,
    ) -> ExecutedTransaction {
        let gas_ref = ctx.current_object_ref(gas_obj_id).await;
        let builder = ctx.get_grpc_client().transaction_builder();
        let data = builder
            .split_coin(signer, primary_coin, amounts, Some(gas_ref.0), 20_000_000)
            .await
            .unwrap();
        ctx.sign_and_execute(data, "coin split").await
    }
}
