// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext};
use async_trait::async_trait;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::governance::StakedSui;
use tracing::info;

pub struct StakingTest;

#[async_trait]
impl TestCaseImpl for StakingTest {
    fn name(&self) -> &'static str {
        "Staking"
    }

    fn description(&self) -> &'static str {
        "Stake SUI with a validator and verify the created StakedSui object"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        info!("Testing staking workflow");

        let sender = ctx.get_wallet_address();
        // Fund two coins: one to stake, one to pay for gas. Both refs are
        // supplied explicitly so the transaction builder stays on
        // `LedgerService` (no gas enumeration over `StateService`).
        let coins = ctx.get_sui_from_faucet(Some(2)).await;
        let stake_coin_id = *coins[0].id();
        let gas_coin_id = *coins[1].id();

        // Pick a validator from the gRPC system-state summary and remember its
        // staking pool so we can correlate the created StakedSui object.
        let system_state = ctx.get_latest_sui_system_state().await;
        let current_epoch = system_state.epoch;
        let validator = system_state
            .active_validators
            .first()
            .expect("Should have at least one active validator");
        let validator_addr = validator.sui_address;
        let validator_pool_id = validator.staking_pool_id;
        info!("Staking to validator: {validator_addr} (pool {validator_pool_id})");

        let gas_price = ctx.get_reference_gas_price().await;
        let gas_ref = ctx.current_object_ref(gas_coin_id).await;
        let stake_ref = ctx.current_object_ref(stake_coin_id).await;

        let data = TestTransactionBuilder::new(sender, gas_ref, gas_price)
            .call_staking(stake_ref, validator_addr)
            .build();
        let response = ctx.sign_and_execute(data, "staking transaction").await;

        // Identify the created StakedSui object from the effects, then fetch and
        // decode it natively (`LedgerService`).
        let created = response.effects.created();
        let mut staked_sui = None;
        for (obj_ref, _owner) in &created {
            let object = ctx.get_grpc_client().get_object(obj_ref.0).await?;
            if let Ok(stake) = StakedSui::try_from(&object) {
                staked_sui = Some((object, stake));
                break;
            }
        }
        let (object, stake) = staked_sui.expect("Staking should create a StakedSui object");

        // Owner is the staker.
        assert_eq!(
            object.owner(),
            &sui_types::object::Owner::AddressOwner(sender),
            "StakedSui should be owned by the staker",
        );
        // Principal matches the staked coin value.
        assert_eq!(
            stake.principal(),
            coins[0].value(),
            "StakedSui principal should equal the staked coin value",
        );
        // Staking pool matches the chosen validator.
        assert_eq!(
            stake.pool_id(),
            validator_pool_id,
            "StakedSui pool should match the validator's staking pool",
        );
        // A newly requested stake activates in the NEXT epoch, not the current
        // one — it must not be reported as active now.
        assert_eq!(
            stake.activation_epoch(),
            current_epoch + 1,
            "Newly requested stake should activate next epoch, not the current one",
        );
        info!(
            "Staking verified: StakedSui {} principal {} pool {} activates epoch {}",
            stake.id(),
            stake.principal(),
            stake.pool_id(),
            stake.activation_epoch(),
        );

        Ok(())
    }
}
