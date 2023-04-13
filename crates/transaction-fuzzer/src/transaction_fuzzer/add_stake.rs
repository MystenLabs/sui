// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::*;

pub struct RequestAddStakeGen;

pub struct RequestAddStake {
    sender: SuiAddress,
    stake_amount: u64,
    staked_with: SuiAddress,
}

impl GenStateChange for RequestAddStakeGen {
    fn create(&self, runner: &mut FuzzTestRunner) -> Option<Box<dyn StatePredicate>> {
        let stake_amount = runner
            .rng
            .gen_range(MIN_DELEGATION_AMOUNT..=MAX_DELEGATION_AMOUNT);
        let staked_with = runner.pick_random_active_validator().sui_address;
        let sender = runner.pick_random_sender();
        Some(Box::new(RequestAddStake {
            sender,
            stake_amount,
            staked_with,
        }))
    }
}

#[async_trait]
impl StatePredicate for RequestAddStake {
    async fn run(&mut self, runner: &mut FuzzTestRunner) -> Result<TransactionEffects> {
        println!("REQUEST ADD STAKE");
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder
                .obj(ObjectArg::SharedObject {
                    id: SUI_SYSTEM_STATE_OBJECT_ID,
                    initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                    mutable: true,
                })
                .unwrap();
            builder.pure(self.staked_with).unwrap();
            let coin = FuzzTestRunner::split_off(&mut builder, self.stake_amount);
            move_call! {
                builder,
                (SUI_SYSTEM_OBJECT_ID)::sui_system::request_add_stake(Argument::Input(0), coin, Argument::Input(1))
            };
            builder.finish()
        };
        let effects = runner.sign_and_run_txn(self.sender, pt).await;

        Ok(effects)
    }

    async fn pre_epoch_post_condition(
        &mut self,
        runner: &mut FuzzTestRunner,
        effects: &TransactionEffects,
    ) {
        // Adding stake should always succeed since we're above the staking threshold
        assert!(effects.status().is_ok());
        // Assert that a `StakedSui` object matching the amount delegated is created.
        // Assert that this staked sui
        let object = runner
            .get_created_object_of_type_name(effects, "StakedSui")
            .await
            .unwrap();
        let epoch = runner.system_state().epoch;
        runner.accounts.get_mut(&self.sender).unwrap().add_stake(
            self.staked_with,
            object.id(),
            self.stake_amount,
            epoch,
        );
        println!("Staked: {}", object.id());
        let staked_amount =
            object.get_total_sui(&runner.db().await).unwrap() - object.storage_rebate;
        assert_eq!(staked_amount, self.stake_amount);
        assert_eq!(object.owner.get_owner_address().unwrap(), self.sender);
        runner.display_effects(effects);
    }

    async fn post_epoch_post_condition(&mut self, _runner: &mut FuzzTestRunner) {}
}
