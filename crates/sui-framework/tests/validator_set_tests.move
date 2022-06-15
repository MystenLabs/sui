// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::validator_set_tests {
    use sui::balance;
    use sui::coin;
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use sui::validator::{Self, Validator};
    use sui::validator_set;

    #[test]
    fun test_validator_set_flow() {
        // Create 4 validators, with stake 100, 200, 300, 400.
        let (ctx1, validator1) = create_validator(@0x1, 1);
        let (_ctx2, validator2) = create_validator(@0x2, 2);
        let (_ctx3, validator3) = create_validator(@0x3, 3);
        let (_ctx4, validator4) = create_validator(@0x4, 4);

        // Create a validator set with only the first validator in it.
        let validator_set = validator_set::new(vector[validator1]);
        assert!(validator_set::total_validator_candidate_count(&validator_set) == 1, 0);
        assert!(validator_set::validator_stake(&validator_set) == 100, 0);

        // Add the other 3 validators one by one.
        validator_set::request_add_validator(
            &mut validator_set,
            validator2,
        );
        // Adding validator during the epoch should not affect stake and quorum threshold.
        assert!(validator_set::total_validator_candidate_count(&validator_set) == 2, 0);
        assert!(validator_set::validator_stake(&validator_set) == 100, 0);

        validator_set::request_add_validator(
            &mut validator_set,
            validator3,
        );
        validator_set::request_add_stake(
            &mut validator_set,
            coin::into_balance(coin::mint_for_testing(500, &mut ctx1)),
            &ctx1,
        );
        // Adding stake to existing active validator during the epoch
        // should not change total stake.
        assert!(validator_set::validator_stake(&validator_set) == 100, 0);

        validator_set::request_withdraw_stake(
            &mut validator_set,
            500,
            100 /* min_validator_stake */,
            &ctx1,
        );
        assert!(validator_set::validator_stake(&validator_set) == 100, 0);

        validator_set::request_add_validator(
            &mut validator_set,
            validator4,
        );

        let reward = balance::zero<SUI>();
        validator_set::advance_epoch(&mut validator_set, &mut reward, &mut ctx1);
        // The total stake and quorum should reflect 4 validators.
        assert!(validator_set::total_validator_candidate_count(&validator_set) == 4, 0);
        assert!(validator_set::validator_stake(&validator_set) == 1000, 0);

        validator_set::request_remove_validator(
            &mut validator_set,
            &ctx1,
        );
        // Total validator candidate count changes, but total stake remains during epoch.
        assert!(validator_set::total_validator_candidate_count(&validator_set) == 3, 0);
        assert!(validator_set::validator_stake(&validator_set) == 1000, 0);
        validator_set::advance_epoch(&mut validator_set, &mut reward, &mut ctx1);
        // Validator1 is gone.
        assert!(validator_set::validator_stake(&validator_set) == 900, 0);

        validator_set::destroy_for_testing(validator_set, &mut ctx1);
        balance::destroy_zero(reward);
    }

    fun create_validator(addr: address, hint: u8): (TxContext, Validator) {
        let stake_value = (hint as u64) * 100;
        let ctx = tx_context::new_from_hint(addr, hint, 0, 0);
        let init_stake = coin::mint_for_testing(stake_value, &mut ctx);
        let init_stake = coin::into_balance(init_stake);
        let validator = validator::new(
            addr,
            vector[hint],
            vector[hint],
            vector[hint],
            init_stake,
        );
        (ctx, validator)
    }
}
