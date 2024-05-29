// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module utils::safe_tests {
    use utils::safe::{Self, Safe, TransferCapability, OwnerCapability};
    use sui::test_scenario::{Self as ts, Scenario, ctx};
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::test_utils;

    const TEST_SENDER_ADDR: address = @0x1;
    const TEST_OWNER_ADDR: address = @0x1337;
    const TEST_DELEGATEE_ADDR: address = @0x1ce1ce1ce;

    fun create_safe(scenario: &mut Scenario, owner: address, stored_amount: u64) {
        scenario.next_tx(owner);
        {
            let coin = coin::mint_for_testing<SUI>(stored_amount, ctx(scenario));
            safe::create(coin, ctx(scenario));
        };
    }

    // Delegates the safe to delegatee and return the capability ID.
    fun delegate_safe(scenario: &mut Scenario, owner: address, delegate_to: address, delegate_amount: u64): ID {
        let id;
        scenario.next_tx(owner);
        let mut safe = scenario.take_shared<Safe<SUI>>();
        let cap = scenario.take_from_sender<OwnerCapability<SUI>>();
        let capability = safe.create_transfer_capability(&cap, delegate_amount, ctx(scenario));
        id = object::id(&capability);
        transfer::public_transfer(capability, delegate_to);
        scenario.return_to_sender(cap);
        ts::return_shared(safe);
        id
    }

    fun withdraw_as_delegatee(scenario: &mut Scenario, delegatee: address, withdraw_amount: u64) {
        scenario.next_tx(delegatee);
        let mut safe = scenario.take_shared<Safe<SUI>>();
        let mut capability = scenario.take_from_sender<TransferCapability<SUI>>();
        let balance = safe.debit(&mut capability, withdraw_amount);
        test_utils::destroy(balance);

        scenario.return_to_sender(capability);
        ts::return_shared(safe);
    }

    fun revoke_capability(scenario: &mut Scenario, owner: address, capability_id: ID) {
        scenario.next_tx(owner);
        let mut safe = scenario.take_shared<Safe<SUI>>();
        let cap = scenario.take_from_sender<OwnerCapability<SUI>>();
        safe.revoke_transfer_capability(&cap, capability_id);

        scenario.return_to_sender(cap);
        ts::return_shared(safe);
    }

    #[test]
    /// Ensure that all funds can be withdrawn by the owners
    fun test_safe_create_and_withdraw_funds_as_owner() {
        let owner = TEST_OWNER_ADDR;
        let mut scenario = ts::begin(TEST_SENDER_ADDR);

        let initial_funds = 1000u64;
        create_safe(scenario, owner, initial_funds);

        scenario.next_tx(owner);
        let mut safe = scenario.take_shared<Safe<SUI>>();
        let cap = scenario.take_from_sender<OwnerCapability<SUI>>();

        safe.withdraw(&cap, initial_funds, ts::ctx(scenario));
        scenario.next_tx(owner);
        let withdrawn_coin = scenario.take_from_sender<Coin<SUI>>();
        assert!(withdrawn_coin.value() == initial_funds, 0);

        test_utils::destroy(withdrawn_coin);
        scenario.return_to_sender(cap);
        ts::return_shared(safe);

        scenario.end();
    }

    #[test]
    /// Ensure that all funds can be withdrawn to a delegator
    fun test_safe_create_and_withdraw_funds_as_delegatee() {
        let owner = TEST_OWNER_ADDR;
        let delegatee = TEST_DELEGATEE_ADDR;
        let mut scenario = ts::begin(TEST_SENDER_ADDR);

        let initial_funds = 1000u64;
        let delegated_funds = 1000u64;
        // Create Safe
        create_safe(scenario, owner, initial_funds);
        delegate_safe(scenario, owner, delegatee, delegated_funds);
        withdraw_as_delegatee(scenario, delegatee, delegated_funds);

        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = safe::EOverdrawn)]
    /// Ensure that funds cannot be over withdrawn
    fun test_safe_attempt_to_over_withdraw() {
        let owner = TEST_OWNER_ADDR;
        let delegatee = TEST_DELEGATEE_ADDR;
        let mut scenario = ts::begin(TEST_SENDER_ADDR);

        let initial_funds = 1000u64;
        let delegated_funds = 1000u64;
        // Create Safe
        create_safe(scenario, owner, initial_funds);
        delegate_safe(scenario, owner, delegatee, delegated_funds);

        // Withdraw all funds
        withdraw_as_delegatee(scenario, delegatee, delegated_funds);
        // Attempt to withdraw by 1 coin.
        withdraw_as_delegatee(scenario, delegatee, 1);

        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = safe::ETransferCapabilityRevoked)]
    /// Ensure that funds cannot be over withdrawn
    fun test_safe_withdraw_revoked() {
        let owner = TEST_OWNER_ADDR;
        let delegatee = TEST_DELEGATEE_ADDR;
        let mut scenario = ts::begin(TEST_SENDER_ADDR);

        let initial_funds = 1000u64;
        let delegated_funds = 1000u64;
        // Create Safe
        create_safe(scenario, owner, initial_funds);
        let capability_id = delegate_safe(scenario, owner, delegatee, delegated_funds);

        revoke_capability(scenario, owner, capability_id);

        // Withdraw funds
        withdraw_as_delegatee(scenario, delegatee, delegated_funds);

        scenario.end();
    }

    #[test]
    #[expected_failure(abort_code = safe::ETransferCapabilityRevoked)]
    /// Ensure owner cannot withdraw funds after revoking itself.
    fun test_safe_withdraw_self_revoked() {
        let owner = TEST_OWNER_ADDR;
        let mut scenario = ts::begin(owner);

        let initial_funds = 1000u64;
        create_safe(scenario, owner, initial_funds);

        scenario.next_tx(owner);
        let cap = scenario.take_from_sender<OwnerCapability<SUI>>();
        let mut safe = scenario.take_shared<Safe<SUI>>();
        let mut transfer_capability = safe.create_transfer_capability(&cap, initial_funds, ctx(scenario));
        // Function under test
        safe.self_revoke_transfer_capability(&transfer_capability);
        ts::return_shared(safe);

        // Try withdraw funds with transfer capability.
        scenario.next_tx(owner);
        let mut safe = scenario.take_shared<Safe<SUI>>();
        let balance = safe.debit(&mut transfer_capability, 1000u64);
        test_utils::destroy(balance);

        ts::return_shared(safe);
        scenario.return_to_sender(cap);
        scenario.return_to_sender(transfer_capability);
        scenario.end();
    }
}
