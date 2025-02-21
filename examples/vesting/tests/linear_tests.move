// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module vesting::linear_tests;

use vesting::linear::{Self, new_wallet, Wallet};
use sui::clock::{Self};
use sui::coin::{Self};
use sui::test_scenario as ts;
use sui::sui::SUI;

public struct Token has key, store { id: UID }

const OWNER_ADDR: address = @0xAAAA;
const CONTROLLER_ADDR: address = @0xBBBB;
const FULLY_VESTED_AMOUNT: u64 = 10_000;
const VESTING_DURATION: u64 = 1_000_000;
const START_TIME: u64 = 1_000;

fun test_setup(): ts::Scenario {
    let mut ts = ts::begin(CONTROLLER_ADDR);
    let coins = coin::mint_for_testing<SUI>(FULLY_VESTED_AMOUNT, ts.ctx());
    let now = clock::create_for_testing(ts.ctx());
    let wallet = new_wallet(coins, &now, START_TIME, VESTING_DURATION, ts.ctx());
    transfer::public_transfer(wallet, OWNER_ADDR);
    now.destroy_for_testing();
    ts
}

#[test]
#[expected_failure(abort_code = linear::EInvalidStartTime)]
fun test_invalid_start_time() {
    let mut ts = ts::begin(CONTROLLER_ADDR);
    let coins = coin::mint_for_testing<SUI>(FULLY_VESTED_AMOUNT, ts.ctx());
    let now = clock::create_for_testing(ts.ctx());
    let wallet = new_wallet(coins, &now, 0, VESTING_DURATION, ts.ctx());
    transfer::public_transfer(wallet, OWNER_ADDR);
    now.destroy_for_testing();
    ts::end(ts);
}

#[test]
fun test_linear_vesting() {
    let mut ts = test_setup();
    ts.next_tx(OWNER_ADDR);
    let mut now = clock::create_for_testing(ts.ctx());
    let mut wallet = ts.take_from_sender<Wallet<SUI>>();

    // check zero vested
    now.increment_for_testing(START_TIME);
    assert!(wallet.claimable(&now) == 0);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT);

    // vest half
    now.increment_for_testing(VESTING_DURATION / 2);
    assert!(wallet.claimable(&now) == FULLY_VESTED_AMOUNT / 2);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT);
    let coins = wallet.claim(&now, ts.ctx());
    transfer::public_transfer(coins, OWNER_ADDR);
    assert!(wallet.claimable(&now) == 0);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT / 2);

    // vest in full
    now.set_for_testing(START_TIME + VESTING_DURATION);
    assert!(wallet.claimable(&now) == FULLY_VESTED_AMOUNT / 2);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT / 2);
    let coins = wallet.claim(&now, ts.ctx());
    transfer::public_transfer(coins, OWNER_ADDR);
    assert!(wallet.claimable(&now) == 0);
    assert!(wallet.balance() == 0);

    ts.return_to_sender(wallet);
    now.destroy_for_testing();
    let _end = ts::end(ts);
}
