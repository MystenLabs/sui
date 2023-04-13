// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::*;

pub struct RequestWithdrawStakeGen;

pub struct RequestWithdrawStake {
    pub sender: SuiAddress,
    pub stake_id: ObjectID,
    pub staked_with: SuiAddress,
}

impl GenStateChange for RequestWithdrawStakeGen {
    fn create(&self, runner: &mut FuzzTestRunner) -> Option<Box<dyn StatePredicate>> {
        let sender = runner.pick_random_sender();
        let account = runner.accounts.get(&sender).unwrap();
        if account.staked_with.is_empty() {
            return None;
        }
        let (staked_with, stakes) = account
            .staked_with
            .get_index(runner.rng.gen_range(0..account.staked_with.len()))
            .unwrap();
        assert!(!stakes.is_empty());
        let stake_id = stakes[runner.rng.gen_range(0..stakes.len())];
        Some(Box::new(RequestWithdrawStake {
            sender,
            stake_id,
            staked_with: *staked_with,
        }))
    }
}

#[async_trait]
impl StatePredicate for RequestWithdrawStake {
    async fn run(&mut self, runner: &mut FuzzTestRunner) -> Result<TransactionEffects> {
        println!("REQUEST WITHDRAW STAKE {}", self.stake_id);
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder
                .obj(ObjectArg::SharedObject {
                    id: SUI_SYSTEM_STATE_OBJECT_ID,
                    initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                    mutable: true,
                })
                .unwrap();
            builder
                .obj(ObjectArg::ImmOrOwnedObject(
                    runner.object_reference_for_id(self.stake_id).await,
                ))
                .unwrap();
            move_call! {
                builder,
                (SUI_SYSTEM_OBJECT_ID)::sui_system::request_withdraw_stake(Argument::Input(0), Argument::Input(1))
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
        if effects.status().is_ok() {
            let (stake_amount, _staking_epoch) = {
                let account = runner.accounts.get_mut(&self.sender).unwrap();
                account.remove_stake(self.staked_with, self.stake_id);
                let (stake_amount, staking_epoch) =
                    account.staking_info.get(&self.stake_id).unwrap();
                (*stake_amount, *staking_epoch)
            };
            let object = runner
                .get_created_object_of_type_name(effects, "Coin")
                .await
                .unwrap();
            let return_amount =
                object.get_total_sui(&runner.db().await).unwrap() - object.storage_rebate;
            println!("STAKED: {}, returned: {}", stake_amount, return_amount);
            // assert_eq!(
            //     utils::calculate_rewards(
            //         stake_amount,
            //         staking_epoch,
            //         runner.system_state().epoch,
            //         &runner.pre_reconfiguration_states
            //     )
            //     .unwrap(),
            //     return_amount
            // );
        } else {
            println!("STATUS: {:#?}", effects.status());
        }
        runner.display_effects(effects);
    }

    async fn post_epoch_post_condition(&mut self, _runner: &mut FuzzTestRunner) {}
}
