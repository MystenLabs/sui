// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module vesting::backloaded_tests;

use vesting::backloaded::{Self, new_wallet, Wallet};
use sui::clock::{Self};
use sui::coin::{Self};
use sui::test_scenario as ts;
use sui::sui::SUI;

public struct Token has key, store { id: UID }

const OWNER_ADDR: address = @0xAAAA;
const CONTROLLER_ADDR: address = @0xBBBB;
const FULLY_VESTED_AMOUNT: u64 = 10_000;
const START_FRONT: u64 = 1_000;
const START_BACK: u64 = 900_000;
const VESTING_DURATION: u64 = 1_000_000;
const BACK_PERCENTAGE: u8 = 80;

fun test_setup(start_front: u64, start_back: u64, duration: u64, back_percentage: u8): ts::Scenario {
    let mut ts = ts::begin(CONTROLLER_ADDR);
    let coins = coin::mint_for_testing<SUI>(FULLY_VESTED_AMOUNT, ts.ctx());
    let now = clock::create_for_testing(ts.ctx());
    let wallet = new_wallet(coins, &now, start_front, start_back, duration, back_percentage, ts.ctx());
    transfer::public_transfer(wallet, OWNER_ADDR);
    now.destroy_for_testing();
    ts
}

#[test]
#[expected_failure(abort_code = backloaded::EInvalidBackStartTime)]
fun test_invalid_back_start_time() {
    let ts = test_setup(START_FRONT, START_FRONT - 100, VESTING_DURATION, BACK_PERCENTAGE);
    ts::end(ts);
}

#[test]
#[expected_failure(abort_code = backloaded::EInvalidPercentageRange)]
fun test_invalid_percentage_range() {
    let ts = test_setup(START_FRONT, START_BACK, VESTING_DURATION, 150);
    ts::end(ts);
}

#[test]
fun test_backloaded_vesting() {
    let mut ts = test_setup(START_FRONT, START_BACK, VESTING_DURATION, BACK_PERCENTAGE);
    ts.next_tx(OWNER_ADDR);
    let mut now = clock::create_for_testing(ts.ctx());
    let mut wallet = ts.take_from_sender<Wallet<SUI>>();

    // check zero vested
    now.set_for_testing(START_FRONT);
    assert!(wallet.claimable(&now) == 0);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT);

    // vest first half of the first duration
    let front_duration = START_BACK - START_FRONT;
    now.increment_for_testing(front_duration / 2);
    let front_duration_claimable = FULLY_VESTED_AMOUNT * (100 - BACK_PERCENTAGE as u64) / 100;
    assert!(wallet.claimable(&now) == front_duration_claimable / 2);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT);
    let mut coins = wallet.claim(&now, ts.ctx());
    transfer::public_transfer(coins, OWNER_ADDR);
    assert!(wallet.claimable(&now) == 0);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT - (front_duration_claimable / 2));

    // vest the rest of the first duration
    now.increment_for_testing(front_duration / 2);
    assert!(wallet.claimable(&now) == front_duration_claimable / 2);
    coins = wallet.claim(&now, ts.ctx());
    transfer::public_transfer(coins, OWNER_ADDR);
    assert!(wallet.claimable(&now) == 0);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT - front_duration_claimable);

    // vest first half of the last duration
    let back_duration = VESTING_DURATION - front_duration;
    now.increment_for_testing(back_duration / 2);
    let back_duration_claimable = FULLY_VESTED_AMOUNT * (BACK_PERCENTAGE as u64) / 100;
    assert!(wallet.claimable(&now) == back_duration_claimable / 2);
    coins = wallet.claim(&now, ts.ctx());
    transfer::public_transfer(coins, OWNER_ADDR);
    assert!(wallet.claimable(&now) == 0);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT - front_duration_claimable - (back_duration_claimable / 2));

    // vest all the remaining coins
    now.increment_for_testing(back_duration / 2);
    assert!(wallet.claimable(&now) == back_duration_claimable / 2);
    coins = wallet.claim(&now, ts.ctx());
    transfer::public_transfer(coins, OWNER_ADDR);
    assert!(wallet.claimable(&now) == 0);
    assert!(wallet.balance() == 0);

    ts.return_to_sender(wallet);
    now.destroy_for_testing();
    let _end = ts::end(ts);
}

#[test]
fun test_backloaded_claimable() {
    let mut ts = test_setup(START_FRONT, START_FRONT + 100, 200, BACK_PERCENTAGE);
    ts.next_tx(OWNER_ADDR);
    let mut now = clock::create_for_testing(ts.ctx());
    let mut wallet = ts.take_from_sender<Wallet<SUI>>();
    let first_duration_claimable = FULLY_VESTED_AMOUNT * (100 - BACK_PERCENTAGE as u64) / 100;
    let last_duration_claimable = FULLY_VESTED_AMOUNT * (BACK_PERCENTAGE as u64) / 100;

    // check zero vested
    now.increment_for_testing(START_FRONT);
    assert!(wallet.claimable(&now) == 0);
    assert!(wallet.balance() == FULLY_VESTED_AMOUNT);

    // fully vest first_duration_claimable and a quarter of the last_duration_claimable
    now.increment_for_testing(125);
    let coin = wallet.claim(&now, ts.ctx());
    assert!(coin.value() == first_duration_claimable + last_duration_claimable / 4);

    // vest remaining
    now.increment_for_testing(100);
    assert!(wallet.claimable(&now) == FULLY_VESTED_AMOUNT - coin.value());

    sui::test_utils::destroy(coin);
    ts.return_to_sender(wallet);
    now.destroy_for_testing();
    let _end = ts::end(ts);
}
