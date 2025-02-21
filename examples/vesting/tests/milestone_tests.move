// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module vesting::milestone_tests;

use vesting::milestone::{Self, new_wallet, Wallet};
use sui::coin::{Self};
use sui::test_scenario as ts;
use sui::sui::SUI;

public struct Token has key, store { id: UID }

const OWNER_ADDR: address = @0xAAAA;
const CONTROLLED_ADDR: address = @0xBBBB;
const FULLY_VESTED_AMOUNT: u64 = 10_000;

fun test_setup(): ts::Scenario {
    let mut ts = ts::begin(CONTROLLED_ADDR);
    let _setup = ts.next_tx(CONTROLLED_ADDR);
    {
        let coins = coin::mint_for_testing<SUI>(FULLY_VESTED_AMOUNT, ts.ctx());
        new_wallet(coins, OWNER_ADDR, CONTROLLED_ADDR, ts.ctx());
    };
    ts
}

#[test]
#[expected_failure(abort_code = milestone::EOwnerIsController)]
fun test_owner_is_controller() {
    let mut ts = ts::begin(OWNER_ADDR);
    ts.next_tx(OWNER_ADDR);
    let coins = coin::mint_for_testing<SUI>(FULLY_VESTED_AMOUNT, ts.ctx());
    new_wallet(coins, OWNER_ADDR, OWNER_ADDR, ts.ctx());
    ts::end(ts);
}

#[test]
#[expected_failure(abort_code = milestone::EUnauthorizedOwner)]
fun test_claim_by_unauthorized_owner() {
    let mut ts = test_setup();
    ts.next_tx(CONTROLLED_ADDR);
    let mut wallet = ts.take_shared<Wallet<SUI>>();
    let coins = wallet.claim(ts.ctx());
    transfer::public_transfer(coins, CONTROLLED_ADDR);
    ts::return_shared(wallet);
    ts::end(ts);
}

#[test]
#[expected_failure(abort_code = milestone::EUnauthorizedMilestoneController)]
fun test_new_milestone_by_unathorized_controller() {
    let mut ts = test_setup();
    ts.next_tx(OWNER_ADDR);
    let mut wallet = ts.take_shared<Wallet<SUI>>();
    wallet.update_milestone_percentage(50, ts.ctx());
    ts::return_shared(wallet);
    ts::end(ts);
}

#[test]
#[expected_failure(abort_code = milestone::EMilestonePercentageRange)]
fun test_new_invalid_milestone_percentage_range() {
    let mut ts = test_setup();
    ts.next_tx(CONTROLLED_ADDR);
    let mut wallet = ts.take_shared<Wallet<SUI>>();
    wallet.update_milestone_percentage(120, ts.ctx());
    ts::return_shared(wallet);
    ts::end(ts);
}

#[test]
#[expected_failure(abort_code = milestone::EInvalidNewMilestone)]
fun test_new_invalid_milestone_percentage() {
    let mut ts = test_setup();
    ts.next_tx(CONTROLLED_ADDR);
    let mut wallet = ts.take_shared<Wallet<SUI>>();
    wallet.update_milestone_percentage(50, ts.ctx());
    wallet.update_milestone_percentage(30, ts.ctx());
    ts::return_shared(wallet);
    ts::end(ts);
}

#[test]
fun test_milestone_based_vesting() {
    let mut ts = test_setup();

    let _check_zero_vested = ts.next_tx(OWNER_ADDR);
    {
        let wallet = ts.take_shared<Wallet<SUI>>();
        assert!(wallet.claimable() == 0);
        assert!(wallet.balance() == FULLY_VESTED_AMOUNT);
        ts::return_shared(wallet);
    };

    let _vest_half = ts.next_tx(CONTROLLED_ADDR);
    {
        let mut wallet = ts.take_shared<Wallet<SUI>>();
        wallet.update_milestone_percentage(50, ts.ctx());
        ts::return_shared(wallet);
    };
    let _check_half_vested = ts.next_tx(OWNER_ADDR);
    {
        let mut wallet = ts.take_shared<Wallet<SUI>>();
        assert!(wallet.claimable() == FULLY_VESTED_AMOUNT / 2);
        assert!(wallet.balance() == FULLY_VESTED_AMOUNT);
        let coins = wallet.claim(ts.ctx());
        transfer::public_transfer(coins, OWNER_ADDR);
        assert!(wallet.claimable() == 0);
        assert!(wallet.balance() == FULLY_VESTED_AMOUNT / 2);
        ts::return_shared(wallet);
    };

    let _vest_full = ts.next_tx(CONTROLLED_ADDR);
    {
        let mut wallet = ts.take_shared<Wallet<SUI>>();
        wallet.update_milestone_percentage(100, ts.ctx());
        ts::return_shared(wallet);
    };
    let _check_fully_vested = ts.next_tx(OWNER_ADDR);
    {
        let mut wallet = ts.take_shared<Wallet<SUI>>();
        assert!(wallet.claimable() == FULLY_VESTED_AMOUNT / 2);
        assert!(wallet.balance() == FULLY_VESTED_AMOUNT / 2);
        let coins = wallet.claim(ts.ctx());
        transfer::public_transfer(coins, OWNER_ADDR);
        assert!(wallet.claimable() == 0);
        assert!(wallet.balance() == 0);
        ts::return_shared(wallet);
    };

    let _end = ts::end(ts);
}
