// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::coin_balance_tests;

use sui::balance;
use sui::coin;
use sui::pay;
use sui::sui::SUI;
use sui::test_scenario;

public struct TestType has drop {}

#[test]
fun type_morphing() {
    let mut scenario = test_scenario::begin(@0x1);

    let balance = balance::zero<SUI>();
    let coin = balance.into_coin(scenario.ctx());
    let balance = coin.into_balance();

    balance.destroy_zero();

    let mut coin = coin::mint_for_testing<SUI>(100, scenario.ctx());
    let balance_mut = coin::balance_mut(&mut coin);
    let sub_balance = balance_mut.split(50);

    assert!(sub_balance.value() == 50);
    assert!(coin.value() == 50);

    let mut balance = coin.into_balance();
    balance.join(sub_balance);

    assert!(balance.value() == 100);

    let coin = balance.into_coin(scenario.ctx());
    pay::keep(coin, scenario.ctx());
    scenario.end();
}

#[test]
fun create_and_destroy() {
    let amount: u64 = 1_000_000_000_000;
    let scenario = test_scenario::begin(@0xaaa);

    let mut supply = balance::create_supply<TestType>(TestType {});
    let balance = supply.increase_supply(amount);
    let value = supply.decrease_supply(balance);
    assert!(value == amount);
    supply.destroy_zero();
    scenario.end();
}

#[test]
#[expected_failure(abort_code = balance::ENonZero)]
fun destroy_non_zero_fail() {
    let amount: u64 = 1_000_000_000_000;
    let mut scenario = test_scenario::begin(@0xaaa);

    let mut supply = balance::create_supply<TestType>(TestType {});
    let balance = supply.increase_supply(amount);
    let coin = coin::from_balance(balance, scenario.ctx());
    coin.burn_for_testing();
    supply.destroy_zero();
    scenario.end();
}
