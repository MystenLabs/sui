// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext, helper::ObjectChecker};
use async_trait::async_trait;
use sui_sdk::wallet_context::WalletContext;
use sui_test_transaction_builder::{increment_counter, publish_basics_package_and_make_counter};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use tracing::info;

pub struct SharedCounterTest;

#[async_trait]
impl TestCaseImpl for SharedCounterTest {
    fn name(&self) -> &'static str {
        "SharedCounter"
    }

    fn description(&self) -> &'static str {
        "Test publishing basics packages and incrementing Counter (shared object)"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        info!("Testing shared object transactions.");

        let sui_objs = ctx.get_sui_from_faucet(Some(1)).await;
        assert!(!sui_objs.is_empty());

        let wallet_context: &WalletContext = ctx.get_wallet();
        let address = ctx.get_wallet_address();
        let (package_ref, (counter_id, initial_counter_version, _)) =
            publish_basics_package_and_make_counter(wallet_context).await;
        let response = increment_counter(
            wallet_context,
            address,
            None,
            package_ref.0,
            counter_id,
            initial_counter_version,
        )
        .await;
        assert!(
            response.effects.status().is_ok(),
            "Increment counter txn failed: {:?}",
            response.effects.status(),
        );

        response
            .effects
            .input_consensus_objects()
            .iter()
            .find(|o| o.id_and_version().0 == counter_id)
            .expect("Expect obj {counter_id} in shared_objects");

        let counter_version = response
            .effects
            .mutated()
            .iter()
            .find_map(|obj| {
                let Owner::Shared {
                    initial_shared_version,
                } = obj.1
                else {
                    return None;
                };

                if obj.0.0 == counter_id && initial_shared_version == initial_counter_version {
                    Some(obj.0.1)
                } else {
                    None
                }
            })
            .expect("Expect obj {counter_id} in mutated");

        // Verify fullnode observes the txn
        ctx.let_fullnode_sync(vec![response.transaction.digest()], 5)
            .await;

        let counter_object = ObjectChecker::new(counter_id)
            .owner(Owner::Shared {
                initial_shared_version: initial_counter_version,
            })
            .check_into_object(ctx.get_fullnode_client())
            .await;

        assert_eq!(
            counter_object.version, counter_version,
            "Expect sequence number to be 2"
        );

        Ok(())
    }
}
