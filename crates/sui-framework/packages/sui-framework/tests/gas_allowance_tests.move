// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::gas_allowance_tests;

use std::unit_test::assert_eq;
use sui::coin::{Self, Coin};
use sui::gas_allowance::{Self, GasAllowance};
use sui::sui::SUI;
use sui::test_scenario::{Self, Scenario};

const FUNDER: address = @0xF;
const SPENDER: address = @0x5;
const OTHER: address = @0xB;

fun create(scenario: &mut Scenario, amount: u64) {
    gas_allowance::new(
        coin::mint_for_testing<SUI>(amount, scenario.ctx()),
        SPENDER,
        scenario.ctx(),
    );
}

#[test]
fun test_funder_tops_up_and_withdraws_partially() {
    let mut scenario = test_scenario::begin(FUNDER);
    create(&mut scenario, 100);

    scenario.next_tx(FUNDER);
    let mut allowance = scenario.take_shared<GasAllowance>();
    assert_eq!(allowance.funder(), FUNDER);
    assert_eq!(allowance.spender(), SPENDER);
    assert_eq!(allowance.remaining(), 100);
    assert!(!allowance.revoked());
    allowance.top_up(coin::mint_for_testing<SUI>(25, scenario.ctx()), scenario.ctx());
    allowance.withdraw(40, scenario.ctx());
    assert_eq!(allowance.remaining(), 85);
    test_scenario::return_shared(allowance);

    scenario.next_tx(FUNDER);
    let payment = scenario.take_from_sender<Coin<SUI>>();
    assert_eq!(coin::value(&payment), 40);
    coin::burn_for_testing(payment);
    let allowance = scenario.take_shared<GasAllowance>();
    test_scenario::return_shared(allowance);
    scenario.end();
}

#[test]
fun test_revoke_returns_balance_and_leaves_tombstone() {
    let mut scenario = test_scenario::begin(FUNDER);
    create(&mut scenario, 100);

    scenario.next_tx(FUNDER);
    let mut allowance = scenario.take_shared<GasAllowance>();
    allowance.top_up(coin::mint_for_testing<SUI>(25, scenario.ctx()), scenario.ctx());
    allowance.revoke(scenario.ctx());
    assert!(allowance.revoked());
    assert_eq!(allowance.remaining(), 0);
    test_scenario::return_shared(allowance);

    scenario.next_tx(FUNDER);
    let payment = scenario.take_from_sender<Coin<SUI>>();
    assert_eq!(coin::value(&payment), 125);
    coin::burn_for_testing(payment);
    let allowance = scenario.take_shared<GasAllowance>();
    assert!(allowance.revoked());
    assert_eq!(allowance.remaining(), 0);
    test_scenario::return_shared(allowance);
    scenario.end();
}

#[test]
#[expected_failure(abort_code = sui::gas_allowance::ENotFunder)]
fun test_non_funder_cannot_withdraw() {
    let mut scenario = test_scenario::begin(FUNDER);
    create(&mut scenario, 100);

    scenario.next_tx(SPENDER);
    let mut allowance = scenario.take_shared<GasAllowance>();
    allowance.withdraw(1, scenario.ctx());
    test_scenario::return_shared(allowance);
    scenario.end();
}

#[test]
#[expected_failure(abort_code = sui::gas_allowance::ENotFunder)]
fun test_non_funder_cannot_top_up() {
    let mut scenario = test_scenario::begin(FUNDER);
    create(&mut scenario, 100);

    scenario.next_tx(OTHER);
    let mut allowance = scenario.take_shared<GasAllowance>();
    allowance.top_up(coin::mint_for_testing<SUI>(1, scenario.ctx()), scenario.ctx());
    test_scenario::return_shared(allowance);
    scenario.end();
}

#[test]
#[expected_failure(abort_code = sui::gas_allowance::ENotFunder)]
fun test_non_funder_cannot_revoke() {
    let mut scenario = test_scenario::begin(FUNDER);
    create(&mut scenario, 100);

    scenario.next_tx(OTHER);
    let mut allowance = scenario.take_shared<GasAllowance>();
    allowance.revoke(scenario.ctx());
    test_scenario::return_shared(allowance);
    scenario.end();
}

#[test]
#[expected_failure(abort_code = sui::gas_allowance::ERevoked)]
fun test_cannot_top_up_revoked_allowance() {
    let mut scenario = test_scenario::begin(FUNDER);
    create(&mut scenario, 100);

    scenario.next_tx(FUNDER);
    let mut allowance = scenario.take_shared<GasAllowance>();
    allowance.revoke(scenario.ctx());
    test_scenario::return_shared(allowance);

    scenario.next_tx(FUNDER);
    let mut allowance = scenario.take_shared<GasAllowance>();
    allowance.top_up(coin::mint_for_testing<SUI>(1, scenario.ctx()), scenario.ctx());
    test_scenario::return_shared(allowance);
    scenario.end();
}
