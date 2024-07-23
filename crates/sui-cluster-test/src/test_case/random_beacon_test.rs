// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext};
use async_trait::async_trait;
use sui_json_rpc_types::{SuiExecutionStatus, SuiTransactionBlockEffectsAPI};
use sui_sdk::wallet_context::WalletContext;
use sui_test_transaction_builder::{emit_new_random_u128, publish_basics_package};
use tracing::info;

pub struct RandomBeaconTest;

#[async_trait]
impl TestCaseImpl for RandomBeaconTest {
    fn name(&self) -> &'static str {
        "RandomBeacon"
    }

    fn description(&self) -> &'static str {
        "Test publishing basics packages and emitting an event that depends on a random value."
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        let wallet_context: &WalletContext = ctx.get_wallet();
        // Test only if the beacon is enabled.
        if !Self::is_beacon_enabled(wallet_context).await {
            info!("Random beacon is not enabled. Skipping test.");
            return Ok(());
        }

        info!("Testing a transaction that uses Random.");

        let sui_objs = ctx.get_sui_from_faucet(Some(1)).await;
        assert!(!sui_objs.is_empty());

        let package_ref = publish_basics_package(wallet_context).await;

        let response = emit_new_random_u128(wallet_context, package_ref.0).await;
        assert_eq!(
            *response.effects.as_ref().unwrap().status(),
            SuiExecutionStatus::Success,
            "Generate new random value txn failed: {:?}",
            *response.effects.as_ref().unwrap().status()
        );

        // Check that only the expected event was emitted.
        let events = response.events.unwrap();
        assert_eq!(
            1,
            events.data.len(),
            "Expected 1 event, got {:?}",
            events.data.len()
        );
        assert_eq!(
            "RandomU128Event".to_string(),
            events.data[0].type_.name.to_string()
        );

        // Verify fullnode observes the txn
        ctx.let_fullnode_sync(vec![response.digest], 5).await;

        Ok(())
    }
}

impl RandomBeaconTest {
    async fn is_beacon_enabled(wallet_context: &WalletContext) -> bool {
        let client = wallet_context.get_client().await.unwrap();
        let config = client.read_api().get_protocol_config(None).await.unwrap();
        *config.feature_flags.get("random_beacon").unwrap()
    }
}
