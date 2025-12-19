// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::balance_tests;

use std::unit_test::destroy;
use sui::balance;
use sui::coin;
use sui::pay;
use sui::sui::SUI;
use sui::test_scenario;

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

public struct MY_COIN has drop {}

#[test]
fun max_supply() {
    let mut supply = balance::create_supply(MY_COIN {});
    supply.increase_supply(std::u64::max_value!() - 1).destroy_for_testing();
    supply.increase_supply(1).destroy_for_testing();

    destroy(supply);
}

#[test, expected_failure(abort_code = sui::balance::EOverflow)]
fun max_supply_overflow_fail() {
    let mut supply = balance::create_supply(MY_COIN {});
    supply.increase_supply(std::u64::max_value!()).destroy_for_testing();
    supply.increase_supply(1).destroy_for_testing(); // custom error code, not arithmetic error

    abort
}

#[test]
fun test_balance() {
    let mut balance = balance::zero<SUI>();
    let another = balance::create_for_testing(1000);

    balance.join(another);

    assert!(balance.value() == 1000);

    let balance1 = balance.split(333);
    let balance2 = balance.split(333);
    let balance3 = balance.split(334);

    balance.destroy_zero();

    assert!(balance1.value() == 333);
    assert!(balance2.value() == 333);
    assert!(balance3.value() == 334);

    destroy(balance1);
    destroy(balance2);
    destroy(balance3);
}
