// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext};
use async_trait::async_trait;
use sui_test_transaction_builder::make_staking_transaction;
use sui_types::transaction_driver_types::ExecuteTransactionRequestType;
use tracing::info;

pub struct StakingTest;

#[async_trait]
impl TestCaseImpl for StakingTest {
    fn name(&self) -> &'static str {
        "Staking"
    }

    fn description(&self) -> &'static str {
        "Stake SUI with a validator and verify the delegation appears"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        info!("Testing staking workflow");

        ctx.get_sui_from_faucet(Some(1)).await;
        let sender = ctx.get_wallet_address();

        // Get a validator address from system state
        let system_state = ctx.get_latest_sui_system_state().await;
        let validator_addr = system_state
            .active_validators
            .first()
            .expect("Should have at least one active validator")
            .sui_address;
        info!("Staking to validator: {validator_addr}");

        // Build and execute the staking transaction
        let txn = make_staking_transaction(ctx.get_wallet(), validator_addr).await;
        let digest = *txn.digest();

        let client = ctx.clone_fullnode_client();
        client
            .quorum_driver_api()
            .execute_transaction_block(
                txn,
                Default::default(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;
        info!("Staking transaction executed: {digest}");

        // Verify the stake appeared
        let stakes = client.governance_api().get_stakes(sender).await?;
        assert!(
            !stakes.is_empty(),
            "Should have at least one stake after staking"
        );
        let matching = stakes
            .iter()
            .find(|s| s.validator_address == validator_addr);
        assert!(
            matching.is_some(),
            "Should find a stake delegated to {validator_addr}"
        );
        info!(
            "Staking verified: {} delegation(s), staked to {validator_addr}",
            stakes.len()
        );

        Ok(())
    }
}
