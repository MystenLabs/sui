// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::safe_tests {
    use sui::safe::{Self, Safe, TransferCapability, OwnerCapability};
    use sui::test_scenario::{Self as ts, Scenario, ctx};
    use sui::coin;
    use sui::object::{Self, ID};
    use sui::balance;
    use sui::sui::SUI;
    use sui::transfer;

    fun create_safe(scenario: &mut Scenario, owner: address, stored_amount: u64) {
        ts::next_tx(scenario, owner);
        {
            let coin = coin::mint_for_testing<SUI>(stored_amount, ctx(scenario));
            safe::create(coin, ctx(scenario));
        };
    }

    // Delegates the safe to delegatee and return the capability ID.
    fun delegate_safe(scenario: &mut Scenario, owner: address, delegate_to: address, delegate_amount: u64): ID {
        let id;
        ts::next_tx(scenario, owner);
        let safe = ts::take_shared<Safe<SUI>>(scenario);
        let cap = ts::take_from_sender<OwnerCapability<SUI>>(scenario);
        let capability = safe::create_transfer_capability(&mut safe, &cap, delegate_amount, ctx(scenario));
        id = object::id(&capability);
        transfer::transfer(capability, delegate_to);
        ts::return_to_sender(scenario, safe);
        ts::return_shared(cap);
        id
    }

    fun withdraw_as_delegatee(scenario: &mut Scenario, delegatee: address, withdraw_amount: u64) {
        ts::next_tx(scenario, delegatee);
        let safe = ts::take_shared<Safe<SUI>>(scenario);
        let capability = ts::take_from_sender<TransferCapability<SUI>>(scenario);
        let balance = safe::debit(&mut safe, &mut capability, withdraw_amount);
        balance::destroy_for_testing(balance);

        ts::return_to_sender(scenario, safe);
        ts::return_shared(capability);
    }

    fun revoke_capability(scenario: &mut Scenario, owner: address, capability_id: ID) {
        ts::next_tx(scenario, owner);
        let safe = ts::take_shared<Safe<SUI>>(scenario);
        let cap = ts::take_from_sender<OwnerCapability<SUI>>(scenario);
        safe::revoke_transfer_capability(&mut safe, &cap, capability_id);

        ts::return_to_sender(scenario, safe);
        ts::return_shared(cap);
    }

    #[test]
    /// Ensure that all funds can be withdrawn by the owners
    fun test_safe_create_and_withdraw_funds_as_owner() {
        let owner = @1337;
        let scenario = ts::begin(@0x1);

        let initial_funds = 1000u64;
        create_safe(&mut scenario, owner, initial_funds);

        ts::next_tx(&mut scenario, owner);
        let safe = ts::take_shared<Safe<SUI>>(&mut scenario);
        let cap = ts::take_from_sender<OwnerCapability<SUI>>(&mut scenario);

        let balance = safe::withdraw_(&mut safe, &cap, initial_funds);
        balance::destroy_for_testing(balance);
        
        ts::return_to_sender(&mut scenario, cap);
        ts::return_shared(safe);


        ts::end(scenario);
    }

    #[test]
    /// Ensure that all funds can be withdrawn to a delegator
    fun test_safe_create_and_withdraw_funds_as_delegatee() {
        let owner = @0x1337;
        let delegatee = @0x1ce1ce1ce;
        let scenario = ts::begin(@0x1);

        let initial_funds = 1000u64;
        let delegated_funds = 1000u64;
        // Create Safe
        create_safe(&mut scenario, owner, initial_funds);
        delegate_safe(&mut scenario, owner, delegatee, delegated_funds);
        withdraw_as_delegatee(&mut scenario, delegatee, delegated_funds);
        ts::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = 3)]
    /// Ensure that funds cannot be over withdrawn
    fun test_safe_attempt_to_over_withdraw() {
        let owner = @0x1337;
        let delegatee = @0x1ce1ce1ce;
        let scenario = ts::begin(@0x1);

        let initial_funds = 1000u64;
        let delegated_funds = 1000u64;
        // Create Safe
        create_safe(&mut scenario, owner, initial_funds);
        delegate_safe(&mut scenario, owner, delegatee, delegated_funds);

        // Withdraw all funds
        withdraw_as_delegatee(&mut scenario, delegatee, delegated_funds);
        // Attempt to withdraw by 1 coin.
        withdraw_as_delegatee(&mut scenario, delegatee, 1);

        ts::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = 2)]
    /// Ensure that funds cannot be over withdrawn
    fun test_safe_withdraw_revoked() {
        let owner = @0x1337;
        let delegatee = @0x1ce1ce1ce;
        let scenario = ts::begin(@0x1);

        let initial_funds = 1000u64;
        let delegated_funds = 1000u64;
        // Create Safe
        create_safe(&mut scenario, owner, initial_funds);
        let capability_id = delegate_safe(&mut scenario, owner, delegatee, delegated_funds);

        revoke_capability(&mut scenario, owner, capability_id);

        // Withdraw funds
        withdraw_as_delegatee(&mut scenario, delegatee, delegated_funds);

        ts::end(scenario);
    }
}
