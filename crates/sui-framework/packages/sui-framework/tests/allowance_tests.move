// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
// `test_tx!` hands bodies a `&mut Clock`; expansions that never touch time
// trip unused_mut_ref per call site, and an allow on the macro has no effect.
#[allow(unused_mut_ref)]
module sui::allowance_tests;

use std::string::String;
use sui::allowance::{Self, Allowance, AllowanceCap};
use sui::balance::Balance;
use sui::clock::{Self, Clock};
use sui::test_scenario::{Self as ts, Scenario};

/// Coin-like marker; allowances under test are `Allowance<Balance<TEST>>`.
public struct TEST has store {}

/// Controlling-app marker; defined here so the test module can create `Permit<APP>`.
public struct APP {}

/// A second app marker for wrong-app tests.
public struct APP2 {}

const FUNDER: address = @0xF;
const SPENDER: address = @0x5;
const SPENDER2: address = @0x52;

// Assertions are behavioral: a check holds iff the spend succeeds or aborts
// as expected. The test `Clock` starts at 0.

// === Test harness ===

/// Wraps a test body in a scenario (begun as `$sender`) with a clock, and
/// cleans both up after.
macro fun test_tx($sender: address, $body: |&mut Scenario, &mut Clock|) {
    let mut scenario = ts::begin($sender);
    let mut clock = clock::create_for_testing(scenario.ctx());
    $body(&mut scenario, &mut clock);
    clock.destroy_for_testing();
    scenario.end();
}

// === Issuance builder ===

/// Readable issuance: `new_allowance().lifetime_cap(1000).create<Balance<TEST>>(ctx)`.
/// Spender defaults to SPENDER, expiry to far-future (the expiration
/// invariant); everything else defaults to unset.
public struct AllowanceBuilder has drop {
    name: String,
    spender: address,
    cap: Option<u256>,
    start_ms: Option<u64>,
    expiry_ms: Option<u64>,
    rate_period_ms: Option<u64>,
    rate_amount: Option<u256>,
}

fun new_allowance(): AllowanceBuilder {
    AllowanceBuilder {
        name: b"test allowance".to_string(),
        spender: SPENDER,
        cap: option::none(),
        start_ms: option::none(),
        expiry_ms: option::some(std::u64::max_value!()),
        rate_period_ms: option::none(),
        rate_amount: option::none(),
    }
}

fun named(mut self: AllowanceBuilder, name: String): AllowanceBuilder {
    self.name = name;
    self
}

fun lifetime_cap(mut self: AllowanceBuilder, cap: u256): AllowanceBuilder {
    self.cap = option::some(cap);
    self
}

fun starts_at_ms(mut self: AllowanceBuilder, ms: u64): AllowanceBuilder {
    self.start_ms = option::some(ms);
    self
}

fun expires_at_ms(mut self: AllowanceBuilder, ms: u64): AllowanceBuilder {
    self.expiry_ms = option::some(ms);
    self
}

/// Period and amount together; the module rejects one without the other.
fun rate_limit(mut self: AllowanceBuilder, period_ms: u64, amount: u256): AllowanceBuilder {
    self.rate_period_ms = option::some(period_ms);
    self.rate_amount = option::some(amount);
    self
}

/// Issue an `Allowance<T>` through the real `new` entry function (shares the
/// allowance, sends the cap to the tx sender).
fun create<T>(self: AllowanceBuilder, ctx: &mut TxContext) {
    let AllowanceBuilder { name, spender, cap, start_ms, expiry_ms, rate_period_ms, rate_amount } =
        self;
    allowance::new<T>(
        name,
        spender,
        cap,
        start_ms,
        expiry_ms,
        rate_period_ms,
        rate_amount,
        ctx,
    );
}

/// Same, app-bound to `A` through `new_for_app`.
fun create_for_app<T, A>(self: AllowanceBuilder, ctx: &mut TxContext) {
    let AllowanceBuilder { name, spender, cap, start_ms, expiry_ms, rate_period_ms, rate_amount } =
        self;
    allowance::new_for_app<T, A>(
        name,
        spender,
        cap,
        start_ms,
        expiry_ms,
        rate_period_ms,
        rate_amount,
        ctx,
    );
}

// === Spend helpers ===

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

// === Tests ===

#[test]
fun test_signer_spend_within_lifetime_cap() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // 400 + 600 == cap: both succeed (cap is inclusive).
        spend(&mut alw, id, FUNDER, 400, clock, scenario.ctx());
        spend(&mut alw, id, FUNDER, 600, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_dropped_withdrawal_consumes_nothing() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // Dropping an unspent withdrawal must not touch the accounting: the
        // full cap is still spendable afterwards.
        let _dropped = allowance::new_withdrawal_for_testing<Balance<TEST>>(id, FUNDER, 1000);
        spend(&mut alw, id, FUNDER, 1000, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsLifetimeCap)]
fun test_lifetime_cap_accumulates() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(500).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        spend(&mut alw, id, FUNDER, 400, clock, scenario.ctx());
        // 400 + 200 > 500: the cap tracks cumulative spend -> aborts.
        spend(&mut alw, id, FUNDER, 200, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENotSpender)]
fun test_wrong_spender_rejected() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(@0xBAD);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        spend(&mut alw, id, FUNDER, 100, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENotStarted)]
fun test_spend_before_start_rejected() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).starts_at_ms(100).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        // t=0 < start=100 -> aborts.
        spend(&mut alw, id, FUNDER, 100, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_spend_at_start_allowed() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).starts_at_ms(100).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        // Start is inclusive: t == start spends.
        clock.set_for_testing(100);
        spend(&mut alw, id, FUNDER, 100, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExpired)]
fun test_spend_after_expiry_rejected() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).expires_at_ms(100).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        // Expiry is inclusive, so t=101 > expiry=100 -> aborts.
        clock.set_for_testing(101);
        spend(&mut alw, id, FUNDER, 100, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_rate_limit_window_resets() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().rate_limit(100, 500).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // t=0: fill the window.
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        // t=150: succeeds only if the elapsed window reset -- 500 + 300 would abort.
        clock.set_for_testing(150);
        spend(&mut alw, id, FUNDER, 300, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_rate_limit_exceeded_in_window() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().rate_limit(100, 500).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        spend(&mut alw, id, FUNDER, 400, clock, scenario.ctx());
        // same window (t=50): 400 + 400 > 500 -> aborts.
        clock.set_for_testing(50);
        spend(&mut alw, id, FUNDER, 400, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_app_spend_and_rotate() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create_for_app<Balance<TEST>, APP>(scenario.ctx());

        // App spends on the allowance's behalf; the sender is irrelevant on
        // this path.
        scenario.next_tx(@0xBEEF);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        spend_as_app(&mut alw, id, 100, clock);

        // App rotates the sign-time gate key; the app path keeps spending.
        alw.rotate_spender(allowance::permit(internal::permit<APP>()), SPENDER2);
        spend_as_app(&mut alw, id, 50, clock);
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EHasApp)]
fun test_signer_spend_rejected_when_app_bound() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create_for_app<Balance<TEST>, APP>(scenario.ctx());

        // Even the designated spender cannot bypass the app via the signer path.
        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        spend(&mut alw, id, FUNDER, 100, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EWrongFunder)]
fun test_wrong_funder_rejected() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        // A withdrawal debiting someone other than the funder must not be
        // released, even when bound to the right allowance.
        spend(&mut alw, id, @0xBAD, 100, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EWrongAllowance)]
fun test_withdrawal_bound_to_other_allowance_rejected() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(FUNDER);
        let first = ts::most_recent_id_shared<Allowance<Balance<TEST>>>().destroy_some();
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        // A potato issued for the first allowance must not spend from the
        // second, even with the right funder and funds type.
        let second = ts::most_recent_id_shared<Allowance<Balance<TEST>>>().destroy_some();
        let mut alw = ts::take_shared_by_id<Allowance<Balance<TEST>>>(scenario, second);
        spend(&mut alw, first, FUNDER, 100, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENameTooLong)]
fun test_name_too_long_rejected() {
    test_tx!(FUNDER, |scenario, _clock| {
        let mut name = b"".to_string();
        129u8.do!(|_| name.append(b"x".to_string()));
        // 128 bytes is the inclusive limit; 129 aborts.
        new_allowance()
            .named(name.substring(0, 128))
            .lifetime_cap(1000)
            .create<Balance<TEST>>(scenario.ctx());
        new_allowance()
            .named(name)
            .lifetime_cap(1000)
            .create<Balance<TEST>>(scenario.ctx());
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENoLimit)]
fun test_no_limit_rejected() {
    test_tx!(FUNDER, |scenario, _clock| {
        new_allowance().create<Balance<TEST>>(scenario.ctx());
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENoExpiration)]
fun test_cap_only_without_expiration_rejected() {
    test_tx!(FUNDER, |scenario, _clock| {
        allowance::new<Balance<TEST>>(
            b"test allowance".to_string(),
            SPENDER,
            option::some(1000),
            option::none(),
            option::none(),
            option::none(),
            option::none(),
            scenario.ctx(),
        );
    });
}

#[test]
fun test_rate_only_without_expiration_ok() {
    test_tx!(FUNDER, |scenario, _clock| {
        allowance::new<Balance<TEST>>(
            b"test allowance".to_string(),
            SPENDER,
            option::none(),
            option::none(),
            option::none(),
            option::some(100),
            option::some(500),
            scenario.ctx(),
        );
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EBadRateLimit)]
fun test_one_sided_rate_limit_rejected() {
    test_tx!(FUNDER, |scenario, _clock| {
        // A period without an amount; the builder cannot express this shape.
        allowance::new<Balance<TEST>>(
            b"test allowance".to_string(),
            SPENDER,
            option::none(),
            option::none(),
            option::none(),
            option::some(100),
            option::none(),
            scenario.ctx(),
        );
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EBadRateLimit)]
fun test_zero_rate_period_rejected() {
    test_tx!(FUNDER, |scenario, _clock| {
        new_allowance().rate_limit(0, 500).create<Balance<TEST>>(scenario.ctx());
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EZeroLifetimeCap)]
fun test_zero_lifetime_cap_rejected() {
    test_tx!(FUNDER, |scenario, _clock| {
        new_allowance().lifetime_cap(0).create<Balance<TEST>>(scenario.ctx());
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EBadTimeWindow)]
fun test_expiry_before_start_rejected() {
    test_tx!(FUNDER, |scenario, _clock| {
        new_allowance()
            .lifetime_cap(1000)
            .starts_at_ms(100)
            .expires_at_ms(50)
            .create<Balance<TEST>>(scenario.ctx());
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EWrongApp)]
fun test_wrong_app_permit_rejected() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create_for_app<Balance<TEST>, APP>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        // A permit for a different app than the allowance is bound to.
        let b = alw.spend_balance_as_app(
            allowance::permit(internal::permit<APP2>()),
            allowance::new_withdrawal_for_testing<Balance<TEST>>(id, FUNDER, 100),
            clock,
        );
        b.destroy_for_testing();
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENoApp)]
fun test_app_spend_on_plain_allowance_rejected() {
    test_tx!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        spend_as_app(&mut alw, id, 100, clock);
        ts::return_shared(alw);
    });
}

#[test]
fun test_funder_revokes() {
    test_tx!(FUNDER, |scenario, _clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        // Issuance sent the cap to the funder; possessing it authorizes the
        // revoke.
        scenario.next_tx(FUNDER);
        let cap = scenario.take_from_sender<AllowanceCap<Balance<TEST>>>();
        let alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        alw.revoke(cap);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EWrongCap)]
fun test_revoke_with_wrong_cap() {
    test_tx!(FUNDER, |scenario, _clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        // Take the first allowance's cap, then issue a second allowance.
        scenario.next_tx(FUNDER);
        let cap = scenario.take_from_sender<AllowanceCap<Balance<TEST>>>();
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        // The first cap must not revoke the second allowance.
        scenario.next_tx(FUNDER);
        let second = ts::most_recent_id_shared<Allowance<Balance<TEST>>>().destroy_some();
        let alw = ts::take_shared_by_id<Allowance<Balance<TEST>>>(scenario, second);
        alw.revoke(cap);
    });
}
