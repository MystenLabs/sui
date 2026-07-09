// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::allowance_tests;

use sui::allowance::{Self, Allowance};
use sui::balance::Balance;
use sui::clock::{Self, Clock};
use sui::test_scenario as ts;

/// Coin-like marker; allowances under test are `Allowance<Balance<TEST>>`.
public struct TEST has store {}

/// Controlling-app marker; defined here so the test module can create `Permit<APP>`.
public struct APP {}

const FUNDER: address = @0xF;
const SPENDER: address = @0x5;
const SPENDER2: address = @0x52;

// Assertions are behavioral: a check holds iff the spend succeeds or aborts
// as expected. The test `Clock` starts at 0.

/// Spends `amount` through the real `spend_balance` and discards the funds.
fun spend(
    alw: &mut Allowance<Balance<TEST>>,
    id: ID,
    funder: address,
    amount: u256,
    clock: &Clock,
    ctx: &TxContext,
) {
    let b = alw.spend_balance(
        allowance::new_withdrawal_for_testing<Balance<TEST>>(id, funder, amount),
        clock,
        ctx,
    );
    b.destroy_for_testing();
}

/// Same, through the real `spend_balance_as_app`.
fun spend_as_app(alw: &mut Allowance<Balance<TEST>>, id: ID, amount: u256, clock: &Clock) {
    let b = alw.spend_balance_as_app(
        allowance::permit(internal::permit<APP>()),
        allowance::new_withdrawal_for_testing<Balance<TEST>>(id, FUNDER, amount),
        clock,
    );
    b.destroy_for_testing();
}

#[test]
fun test_signer_spend_within_lifetime_cap() {
    let mut scenario = ts::begin(FUNDER);
    let clock = clock::create_for_testing(scenario.ctx());
    allowance::new<Balance<TEST>>(
        SPENDER,
        option::some(1000),
        option::none(),
        option::none(),
        option::none(),
        option::none(),
        scenario.ctx(),
    );

    scenario.next_tx(SPENDER);
    let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
    let id = object::id(&alw);

    // 400 + 600 == cap: both succeed (cap is inclusive).
    spend(&mut alw, id, FUNDER, 400, &clock, scenario.ctx());
    spend(&mut alw, id, FUNDER, 600, &clock, scenario.ctx());

    ts::return_shared(alw);
    clock.destroy_for_testing();
    scenario.end();
}

#[test]
fun test_dropped_withdrawal_consumes_nothing() {
    let mut scenario = ts::begin(FUNDER);
    let clock = clock::create_for_testing(scenario.ctx());
    allowance::new<Balance<TEST>>(
        SPENDER,
        option::some(1000),
        option::none(),
        option::none(),
        option::none(),
        option::none(),
        scenario.ctx(),
    );

    scenario.next_tx(SPENDER);
    let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
    let id = object::id(&alw);

    // Dropping an unspent withdrawal must not touch the accounting: the full
    // cap is still spendable afterwards.
    let _dropped = allowance::new_withdrawal_for_testing<Balance<TEST>>(id, FUNDER, 1000);
    spend(&mut alw, id, FUNDER, 1000, &clock, scenario.ctx());

    ts::return_shared(alw);
    clock.destroy_for_testing();
    scenario.end();
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsLifetimeCap)]
fun test_lifetime_cap_accumulates() {
    let mut scenario = ts::begin(FUNDER);
    let clock = clock::create_for_testing(scenario.ctx());
    allowance::new<Balance<TEST>>(
        SPENDER,
        option::some(500),
        option::none(),
        option::none(),
        option::none(),
        option::none(),
        scenario.ctx(),
    );

    scenario.next_tx(SPENDER);
    let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
    let id = object::id(&alw);
    spend(&mut alw, id, FUNDER, 400, &clock, scenario.ctx());
    // 400 + 200 > 500: the cap tracks cumulative spend -> aborts.
    spend(&mut alw, id, FUNDER, 200, &clock, scenario.ctx());

    ts::return_shared(alw);
    clock.destroy_for_testing();
    scenario.end();
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENotSpender)]
fun test_wrong_spender_rejected() {
    let mut scenario = ts::begin(FUNDER);
    let clock = clock::create_for_testing(scenario.ctx());
    allowance::new<Balance<TEST>>(
        SPENDER,
        option::some(1000),
        option::none(),
        option::none(),
        option::none(),
        option::none(),
        scenario.ctx(),
    );

    scenario.next_tx(@0xBAD);
    let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
    let id = object::id(&alw);
    spend(&mut alw, id, FUNDER, 100, &clock, scenario.ctx());

    ts::return_shared(alw);
    clock.destroy_for_testing();
    scenario.end();
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENotStarted)]
fun test_spend_before_start_rejected() {
    let mut scenario = ts::begin(FUNDER);
    let clock = clock::create_for_testing(scenario.ctx());
    allowance::new<Balance<TEST>>(
        SPENDER,
        option::some(1000),
        option::some(100),
        option::none(),
        option::none(),
        option::none(),
        scenario.ctx(),
    );

    scenario.next_tx(SPENDER);
    let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
    let id = object::id(&alw);
    // t=0 < start=100 -> aborts.
    spend(&mut alw, id, FUNDER, 100, &clock, scenario.ctx());

    ts::return_shared(alw);
    clock.destroy_for_testing();
    scenario.end();
}

#[test]
fun test_spend_at_start_allowed() {
    let mut scenario = ts::begin(FUNDER);
    let mut clock = clock::create_for_testing(scenario.ctx());
    allowance::new<Balance<TEST>>(
        SPENDER,
        option::some(1000),
        option::some(100),
        option::none(),
        option::none(),
        option::none(),
        scenario.ctx(),
    );

    scenario.next_tx(SPENDER);
    let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
    let id = object::id(&alw);
    // Start is inclusive: t == start spends.
    clock.set_for_testing(100);
    spend(&mut alw, id, FUNDER, 100, &clock, scenario.ctx());

    ts::return_shared(alw);
    clock.destroy_for_testing();
    scenario.end();
}

#[test]
fun test_rate_limit_window_resets() {
    let mut scenario = ts::begin(FUNDER);
    let mut clock = clock::create_for_testing(scenario.ctx());
    allowance::new<Balance<TEST>>(
        SPENDER,
        option::none(),
        option::none(),
        option::none(),
        option::some(100),
        option::some(500),
        scenario.ctx(),
    );

    scenario.next_tx(SPENDER);
    let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
    let id = object::id(&alw);

    // t=0: fill the window.
    spend(&mut alw, id, FUNDER, 500, &clock, scenario.ctx());
    // t=150: succeeds only if the elapsed window reset -- 500 + 300 would abort.
    clock.set_for_testing(150);
    spend(&mut alw, id, FUNDER, 300, &clock, scenario.ctx());

    ts::return_shared(alw);
    clock.destroy_for_testing();
    scenario.end();
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_rate_limit_exceeded_in_window() {
    let mut scenario = ts::begin(FUNDER);
    let mut clock = clock::create_for_testing(scenario.ctx());
    allowance::new<Balance<TEST>>(
        SPENDER,
        option::none(),
        option::none(),
        option::none(),
        option::some(100),
        option::some(500),
        scenario.ctx(),
    );

    scenario.next_tx(SPENDER);
    let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
    let id = object::id(&alw);
    spend(&mut alw, id, FUNDER, 400, &clock, scenario.ctx());
    // same window (t=50): 400 + 400 > 500 -> aborts.
    clock.set_for_testing(50);
    spend(&mut alw, id, FUNDER, 400, &clock, scenario.ctx());

    ts::return_shared(alw);
    clock.destroy_for_testing();
    scenario.end();
}

#[test]
fun test_app_spend_and_rotate() {
    let mut scenario = ts::begin(FUNDER);
    let clock = clock::create_for_testing(scenario.ctx());
    allowance::new_for_app<Balance<TEST>, APP>(
        SPENDER,
        option::some(1000),
        option::none(),
        option::none(),
        option::none(),
        option::none(),
        scenario.ctx(),
    );

    // App spends on the allowance's behalf; the sender is irrelevant on this path.
    scenario.next_tx(@0xBEEF);
    {
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        spend_as_app(&mut alw, id, 100, &clock);

        // App rotates the sign-time gate key; the app path keeps spending.
        alw.rotate_spender(allowance::permit(internal::permit<APP>()), SPENDER2);
        spend_as_app(&mut alw, id, 50, &clock);
        ts::return_shared(alw);
    };

    clock.destroy_for_testing();
    scenario.end();
}

#[test]
#[expected_failure(abort_code = sui::allowance::EHasApp)]
fun test_signer_spend_rejected_when_app_bound() {
    let mut scenario = ts::begin(FUNDER);
    let clock = clock::create_for_testing(scenario.ctx());
    allowance::new_for_app<Balance<TEST>, APP>(
        SPENDER,
        option::some(1000),
        option::none(),
        option::none(),
        option::none(),
        option::none(),
        scenario.ctx(),
    );

    // Even the designated spender cannot bypass the app via the signer path.
    scenario.next_tx(SPENDER);
    let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
    let id = object::id(&alw);
    spend(&mut alw, id, FUNDER, 100, &clock, scenario.ctx());

    ts::return_shared(alw);
    clock.destroy_for_testing();
    scenario.end();
}

#[test]
#[expected_failure(abort_code = sui::allowance::EWrongFunder)]
fun test_wrong_funder_rejected() {
    let mut scenario = ts::begin(FUNDER);
    let clock = clock::create_for_testing(scenario.ctx());
    allowance::new<Balance<TEST>>(
        SPENDER,
        option::some(1000),
        option::none(),
        option::none(),
        option::none(),
        option::none(),
        scenario.ctx(),
    );

    scenario.next_tx(SPENDER);
    let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
    let id = object::id(&alw);
    // A withdrawal debiting someone other than the funder must not be released,
    // even when bound to the right allowance.
    spend(&mut alw, id, @0xBAD, 100, &clock, scenario.ctx());

    ts::return_shared(alw);
    clock.destroy_for_testing();
    scenario.end();
}

#[test]
fun test_funder_revokes() {
    let mut scenario = ts::begin(FUNDER);
    allowance::new<Balance<TEST>>(
        SPENDER,
        option::some(1000),
        option::none(),
        option::none(),
        option::none(),
        option::none(),
        scenario.ctx(),
    );

    scenario.next_tx(FUNDER);
    let alw = scenario.take_shared<Allowance<Balance<TEST>>>();
    alw.revoke(scenario.ctx());

    scenario.end();
}
