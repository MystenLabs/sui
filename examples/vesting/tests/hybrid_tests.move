// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module vesting::hybrid_tests;

use vesting::hybrid::{new_wallet, Wallet};
use sui::clock::{Self};
use sui::coin::{Self};
use sui::test_scenario as ts;
use sui::sui::SUI;

public struct Token has key, store { id: UID }

const OWNER_ADDR: address = @0xAAAA;
const CONTROLLER_ADDR: address = @0xBBBB;
const FULLY_VESTED_AMOUNT: u64 = 10_000;
const VESTING_DURATION: u64 = 5_000;
const START_TIME: u64 = 1_000;
const CLIFF_TIME: u64 = 2_000;

/// Test setup for hybrid vesting
/// Half of the tokens are cliff vested, and the other half are linearly vested
/// The cliff vesting starts at CLIFF_TIME, and the linear vesting starts at START_TIME
/// At the cliff time, the cliff vested tokens are fully vested and the linearly vested tokens are 1/5 vested
fun test_setup(): ts::Scenario {
    let mut ts = ts::begin(CONTROLLER_ADDR);
    let coins = coin::mint_for_testing<SUI>(FULLY_VESTED_AMOUNT, ts.ctx());
    let now = clock::create_for_testing(ts.ctx());
    let wallet = new_wallet(coins, &now, CLIFF_TIME, START_TIME, VESTING_DURATION, ts.ctx());
    transfer::public_transfer(wallet, OWNER_ADDR);
    now.destroy_for_testing();
    ts
}

#[test]
fun test_hybrid_vesting() {
    let mut ts = test_setup();
    ts.next_tx(OWNER_ADDR);
    let mut now = clock::create_for_testing(ts.ctx());
    let wallet = ts.take_from_sender<Wallet<SUI>>();

    // check claimable amount at start
    now.set_for_testing(START_TIME);
    assert!(wallet.claimable(&now) == 0);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT);

    // check claimable amount at cliff time
    now.set_for_testing(CLIFF_TIME);
    assert!(wallet.claimable(&now) == (FULLY_VESTED_AMOUNT / 2) + (FULLY_VESTED_AMOUNT / 2 / 5));

    // check fully claimable amount after vesting duration
    now.set_for_testing(START_TIME + VESTING_DURATION);
    assert!(wallet.claimable(&now) == FULLY_VESTED_AMOUNT);

    ts.return_to_sender(wallet);
    now.destroy_for_testing();
    let _end = ts::end(ts);
}
