// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module vesting::immediate_tests;

use vesting::linear::{new_wallet, Wallet};
use sui::clock::{Self};
use sui::coin::{Self};
use sui::test_scenario as ts;
use sui::sui::SUI;

public struct Token has key, store { id: UID }

const OWNER_ADDR: address = @0xAAAA;
const CONTROLLER_ADDR: address = @0xBBBB;
const FULLY_VESTED_AMOUNT: u64 = 10_000;
const VESTING_DURATION: u64 = 0;
const START_TIME: u64 = 1;

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
fun test_immediate_vesting() {
    let mut ts = test_setup();
    ts.next_tx(OWNER_ADDR);
    let mut now = clock::create_for_testing(ts.ctx());
    let mut wallet = ts.take_from_sender<Wallet<SUI>>();

    // vest immediately
    now.set_for_testing(START_TIME);
    assert!(wallet.claimable(&now) == FULLY_VESTED_AMOUNT);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT);
    let coins = wallet.claim(&now, ts.ctx());
    transfer::public_transfer(coins, OWNER_ADDR);
    assert!(wallet.claimable(&now) == 0);
    assert!(wallet.balance() == 0);

    ts.return_to_sender(wallet);
    now.destroy_for_testing();
    let _end = ts::end(ts);
}
