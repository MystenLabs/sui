// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
// `test_tx!` hands bodies a `&mut Clock`; expansions that never touch time
// trip unused_mut_ref per call site, and an allow on the macro has no effect.
#[allow(unused_mut_ref)]
module sui::allowance_tests;

use std::string::String;
use sui::allowance::{Self, Allowance, AllowanceCap, RateLimit};
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

const MS_PER_DAY: u64 = 86_400_000;

// Assertions are behavioral: a check holds iff the spend succeeds or aborts
// as expected. The test `Clock` starts at 0.

// === Test harness ===

/// Wraps a test body in a scenario (begun as `$sender`) with a clock, and
/// cleans both up after.
macro fun test($sender: address, $body: |&mut Scenario, &mut Clock|) {
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
    rate_limit: Option<RateLimit>,
}

fun new_allowance(): AllowanceBuilder {
    AllowanceBuilder {
        name: b"test allowance".to_string(),
        spender: SPENDER,
        cap: option::none(),
        start_ms: option::none(),
        expiry_ms: option::some(std::u64::max_value!()),
        rate_limit: option::none(),
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

fun rate_limit(mut self: AllowanceBuilder, rate_limit: RateLimit): AllowanceBuilder {
    self.rate_limit = option::some(rate_limit);
    self
}

/// Issue an `Allowance<T>` through the real `new` entry function (shares the
/// allowance, sends the cap to the tx sender).
fun create<T>(self: AllowanceBuilder, ctx: &mut TxContext) {
    let AllowanceBuilder { name, spender, cap, start_ms, expiry_ms, rate_limit } = self;
    allowance::new<T>(
        name,
        spender,
        cap,
        start_ms,
        expiry_ms,
        rate_limit,
        ctx,
    );
}

/// Same, app-bound to `APP`: proposed by the funder, then accepted by the app.
/// `APP` is fixed rather than generic because `internal::permit` may only be
/// called by the module defining the type.
fun create_for_app<T>(self: AllowanceBuilder, ctx: &mut TxContext) {
    let AllowanceBuilder { name, spender, cap, start_ms, expiry_ms, rate_limit } = self;
    let proposal = allowance::propose_for_app<T, APP>(
        name,
        spender,
        cap,
        start_ms,
        expiry_ms,
        rate_limit,
        ctx,
    );
    allowance::issue<T, APP>(proposal, allowance::settings_permit(internal::permit<APP>()), ctx);
}

// === Spend helpers ===

/// Spends `amount` through the real `balance_spend` and discards the funds.
fun spend(
    alw: &mut Allowance<Balance<TEST>>,
    id: ID,
    funder: address,
    amount: u256,
    clock: &Clock,
    ctx: &TxContext,
) {
    let b = alw.balance_spend(
        allowance::new_withdrawal_for_testing<Balance<TEST>>(id, funder, amount),
        clock,
        ctx,
    );
    b.destroy_for_testing();
}

/// Same, through the real `app_balance_spend`.
fun spend_as_app(
    alw: &mut Allowance<Balance<TEST>>,
    id: ID,
    amount: u256,
    clock: &Clock,
    ctx: &TxContext,
) {
    let b = alw.app_balance_spend(
        allowance::spend_permit(internal::permit<APP>()),
        allowance::new_withdrawal_for_testing<Balance<TEST>>(id, FUNDER, amount),
        clock,
        ctx,
    );
    b.destroy_for_testing();
}

// === Tests ===

#[test]
fun test_signer_spend_within_lifetime_cap() {
    test!(FUNDER, |scenario, clock| {
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
    test!(FUNDER, |scenario, clock| {
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
    test!(FUNDER, |scenario, clock| {
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
    test!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(@0xBAD);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        spend(&mut alw, id, FUNDER, 100, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::ESponsorWithdrawalNotEnabled)]
fun test_sponsor_withdrawal_rejected() {
    test!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        let b = alw.balance_spend(
            allowance::new_sponsor_withdrawal_for_testing<Balance<TEST>>(id, FUNDER, 100),
            clock,
            scenario.ctx(),
        );
        b.destroy_for_testing();
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENotStarted)]
fun test_spend_before_start_rejected() {
    test!(FUNDER, |scenario, clock| {
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
    test!(FUNDER, |scenario, clock| {
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
fun test_spend_at_expiry_rejected() {
    test!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).expires_at_ms(100).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        // Expiry is exclusive: t == expiry already aborts.
        clock.set_for_testing(100);
        spend(&mut alw, id, FUNDER, 100, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_rate_limit_window_resets() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::periodic_rate_limit(100, 500))
            .create<Balance<TEST>>(scenario.ctx());

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
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::periodic_rate_limit(100, 500))
            .create<Balance<TEST>>(scenario.ctx());

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
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_periodic_window_anchors_at_first_spend() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::periodic_rate_limit(100, 500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // The first spend anchors the window at 150, so it runs to 250. t=210
        // is still inside it, even though it crosses a multiple of the period.
        clock.set_for_testing(150);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(210);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_periodic_window_renews_one_period_after_the_anchor() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::periodic_rate_limit(100, 500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // Anchored at 150, so the next window opens at 250, not at 200.
        clock.set_for_testing(150);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(250);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_fixed_window_idle_gap_resets_once() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::periodic_rate_limit(100, 500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        // Ten idle windows grant one fresh window's worth, not ten.
        clock.set_for_testing(1000);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(1001);
        spend(&mut alw, id, FUNDER, 1, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

// Calendar dates below in days from the Unix epoch: 2026-01-01 is day 20454.
// Calendar windows anchor at the first charge and renew on its day-of-month.

#[test]
fun test_calendar_window_monthly_renews_on_anniversary_day() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // First charge 2026-01-15 anchors the cycle; 2026-02-15 renews it.
        clock.set_for_testing(20468 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20499 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_calendar_window_grid_stays_anchored() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // Anchored Jan 15; a late Feb 20 charge lands mid-window without
        // re-anchoring, so Mar 15 still opens a fresh window.
        clock.set_for_testing(20468 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20504 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20527 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_calendar_window_holds_until_anniversary_day() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // 2026-02-14 crosses a month boundary but not the Jan 15 anniversary:
        // same window, 400 + 200 -> aborts.
        clock.set_for_testing(20468 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 400, clock, scenario.ctx());
        clock.set_for_testing(20498 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 200, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_calendar_window_month_boundary_alone_does_not_renew() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // A 2026-01-31 charge must not admit another on 2026-02-01; the
        // window holds until the (clamped) anniversary.
        clock.set_for_testing(20484 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20485 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 1, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_calendar_window_clamps_to_month_end() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // Jan 31 anchor: February renews on the clamped 28th, March back on
        // the 31st.
        clock.set_for_testing(20484 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20512 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20543 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_calendar_window_skipped_window_does_not_shift_the_grid() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // Jan 31 anchor. Spending on Mar 2 uses the window that opened Feb 28,
        // so the next one still starts on Mar 31 -- a late spend does not push
        // the boundary out to Apr 2.
        clock.set_for_testing(20484 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20514 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20543 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_calendar_window_holds_the_day_before_the_anniversary() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // Same Jan 31 / Mar 2 pair, but Mar 30 is still inside that window.
        clock.set_for_testing(20484 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20514 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20542 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 1, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_calendar_window_yearly_holds_across_new_year() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::yearly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // A 2026-12-15 charge runs to its anniversary, not the calendar year:
        // 2027-01-01 is still the same window.
        clock.set_for_testing(20802 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20819 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 1, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_calendar_window_yearly_renews_on_anniversary() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::yearly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // 2026-12-15 -> fresh window on 2027-12-15.
        clock.set_for_testing(20802 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(21167 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_calendar_window_leap_day_anchor_clamps() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::yearly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // Anchored on 2028-02-29: the 2029 anniversary clamps to Feb 28.
        clock.set_for_testing(21243 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(21608 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_calendar_window_quarterly_clamps_across_quarter() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::quarterly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // Anchored 2026-11-30: three months later lands on 2027-02-28 (clamped).
        clock.set_for_testing(20787 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20877 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_calendar_window_two_year_window_holds_at_one_year() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::calendar_rate_limit(24, 500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // `months` is open-ended: a 24-month window has not rolled at one year.
        clock.set_for_testing(20468 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20833 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 1, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

// Leap years and month-end anchors. Day numbers as above: 2028-01-31 is day
// 21214, 2028-02-29 is day 21243.

#[test]
fun test_calendar_window_leap_february_renews_on_the_29th() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // 2028-01-31 anchor: February clamps to the 29th, not the 28th, then
        // March is back on the 31st.
        clock.set_for_testing(21214 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(21243 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(21274 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_calendar_window_leap_february_holds_on_the_28th() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // 2028-01-31 anchor: 2028-02-28 is a day short of the clamped anniversary.
        clock.set_for_testing(21214 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(21242 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 1, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_calendar_window_day_30_anchor_holds_on_the_29th_of_march() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // 2028-01-30 anchor: the Feb 29 clamp does not move the grid to the
        // 29th, so 2028-03-29 is still the February window.
        clock.set_for_testing(21213 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(21243 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(21272 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 1, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_calendar_window_leap_day_anchor_unclamps_in_the_next_leap_year() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::yearly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // 2028-02-29 anchor: 2031 renews on the clamped 28th, 2032 on the 29th.
        clock.set_for_testing(21243 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(22338 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(22704 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_calendar_window_leap_day_anchor_holds_on_feb_28_of_a_leap_year() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::yearly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // 2028-02-29 anchor: 2032-02-28 is still the window opened 2031-02-28.
        clock.set_for_testing(21243 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(22338 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(22703 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 1, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_calendar_window_feb_28_anchor_stays_on_the_28th_in_a_leap_year() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::yearly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // 2027-02-28 anchor: the 2028 anniversary is Feb 28, not the leap day.
        clock.set_for_testing(20877 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(21242 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
fun test_calendar_window_century_february_has_no_leap_day() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // 2100 is not a leap year: a 2100-01-31 anchor renews on 2100-02-28.
        clock.set_for_testing(47512 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(47540 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_calendar_window_twelve_skipped_windows_hold_on_mar_30() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // Same anchor and leap-day charge: 2028-03-30 is still window 11.
        clock.set_for_testing(20908 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(21243 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(21273 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 1, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

// Renewal is day-granular: the time of day of the anchor is irrelevant.

#[test]
fun test_calendar_window_renews_at_midnight_of_the_anniversary() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // Anchored at 2026-01-15 23:59:59.999; 2026-02-15 00:00:00.000 renews.
        clock.set_for_testing(20469 * MS_PER_DAY - 1);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20499 * MS_PER_DAY);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EExceedsRateLimit)]
fun test_calendar_window_holds_until_the_last_millisecond() {
    test!(FUNDER, |scenario, clock| {
        new_allowance()
            .rate_limit(allowance::monthly_rate_limit(500))
            .create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);

        // Same anchor; 2026-02-14 23:59:59.999 is still the first window.
        clock.set_for_testing(20469 * MS_PER_DAY - 1);
        spend(&mut alw, id, FUNDER, 500, clock, scenario.ctx());
        clock.set_for_testing(20499 * MS_PER_DAY - 1);
        spend(&mut alw, id, FUNDER, 1, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

// The date helpers mix bases (Hinnant's internals are 0-based, civil output is
// 1-based, window ordinals are 0-based), so pin the conventions directly.

fun assert_civil(timestamp_ms: u64, year: u64, month: u64, day: u64) {
    let (y, m, d) = allowance::civil_from_ms(timestamp_ms);
    assert!(y == year && m == month && d == day);
}

#[test]
fun test_civil_from_ms_conventions() {
    // Month and day are 1-based: timestamp 0 is 1970-01-01, not (0, 0).
    assert_civil(0, 1970, 1, 1);
    // Days change at 00:00 UTC.
    assert_civil(MS_PER_DAY - 1, 1970, 1, 1);
    assert_civil(MS_PER_DAY, 1970, 1, 2);
    // January / February sit in the mp - 9, year + 1 branch.
    assert_civil(20454 * MS_PER_DAY, 2026, 1, 1);
    assert_civil(21243 * MS_PER_DAY, 2028, 2, 29);
    // March sits in the mp + 3 branch; mid-month checks the day formula.
    assert_civil(20532 * MS_PER_DAY, 2026, 3, 20);
    assert_civil(20453 * MS_PER_DAY, 2025, 12, 31);
}

#[test]
fun test_days_in_month_leap_rules() {
    assert!(allowance::days_in_month(2027, 2) == 28);
    assert!(allowance::days_in_month(2028, 2) == 29);
    // Century years are not leap unless divisible by 400.
    assert!(allowance::days_in_month(2100, 2) == 28);
    assert!(allowance::days_in_month(2000, 2) == 29);
    assert!(allowance::days_in_month(2026, 4) == 30);
    assert!(allowance::days_in_month(2026, 12) == 31);
}

#[test]
fun test_civil_from_ms_century_rules() {
    // 2100 skips the leap day; 2400 keeps it.
    assert_civil(47540 * MS_PER_DAY, 2100, 2, 28);
    assert_civil(47541 * MS_PER_DAY, 2100, 3, 1);
    assert_civil(157113 * MS_PER_DAY, 2400, 2, 29);
}

#[test]
fun test_app_spend_and_rotate() {
    test!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create_for_app<Balance<TEST>>(scenario.ctx());

        // The app permit authorizes the spend, but the tx must still come
        // from the spender.
        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        spend_as_app(&mut alw, id, 100, clock, scenario.ctx());
        alw.rotate_spender(allowance::settings_permit(internal::permit<APP>()), SPENDER2);
        ts::return_shared(alw);

        // Rotation redirects the gate to the new spender.
        scenario.next_tx(SPENDER2);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        spend_as_app(&mut alw, id, 50, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENotSpender)]
fun test_app_spend_wrong_sender_rejected() {
    test!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create_for_app<Balance<TEST>>(scenario.ctx());

        // Even with the app's permit, a non-spender sender cannot spend.
        scenario.next_tx(@0xBEEF);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        spend_as_app(&mut alw, id, 100, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EHasApp)]
fun test_signer_spend_rejected_when_app_bound() {
    test!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create_for_app<Balance<TEST>>(scenario.ctx());

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
    test!(FUNDER, |scenario, clock| {
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
    test!(FUNDER, |scenario, clock| {
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
    test!(FUNDER, |scenario, _clock| {
        let mut name = b"".to_string();
        129u8.do!(|_| name.append(b"x".to_string()));
        // 128 bytes is the inclusive limit; 129 aborts.
        new_allowance()
            .named(name.substring(0, 128))
            .lifetime_cap(1000)
            .create<Balance<TEST>>(scenario.ctx());
        new_allowance().named(name).lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENoLimit)]
fun test_no_limit_rejected() {
    test!(FUNDER, |scenario, _clock| {
        new_allowance().create<Balance<TEST>>(scenario.ctx());
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::ENoExpiration)]
fun test_cap_only_without_expiration_rejected() {
    test!(FUNDER, |scenario, _clock| {
        allowance::new<Balance<TEST>>(
            b"test allowance".to_string(),
            SPENDER,
            option::some(1000),
            option::none(),
            option::none(),
            option::none(),
            scenario.ctx(),
        );
    });
}

#[test]
fun test_rate_only_without_expiration_ok() {
    test!(FUNDER, |scenario, _clock| {
        allowance::new<Balance<TEST>>(
            b"test allowance".to_string(),
            SPENDER,
            option::none(),
            option::none(),
            option::none(),
            option::some(allowance::periodic_rate_limit(100, 500)),
            scenario.ctx(),
        );
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EBadRateLimit)]
fun test_zero_rate_period_rejected() {
    test!(FUNDER, |scenario, _clock| {
        new_allowance()
            .rate_limit(allowance::periodic_rate_limit(0, 500))
            .create<Balance<TEST>>(scenario.ctx());
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EBadRateLimit)]
fun test_zero_rate_amount_rejected() {
    test!(FUNDER, |scenario, _clock| {
        new_allowance()
            .rate_limit(allowance::calendar_rate_limit(1, 0))
            .create<Balance<TEST>>(scenario.ctx());
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EBadRateLimit)]
fun test_zero_calendar_months_rejected() {
    test!(FUNDER, |scenario, _clock| {
        new_allowance()
            .rate_limit(allowance::calendar_rate_limit(0, 500))
            .create<Balance<TEST>>(scenario.ctx());
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EZeroLifetimeCap)]
fun test_zero_lifetime_cap_rejected() {
    test!(FUNDER, |scenario, _clock| {
        new_allowance().lifetime_cap(0).create<Balance<TEST>>(scenario.ctx());
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EBadTimeWindow)]
fun test_expiry_before_start_rejected() {
    test!(FUNDER, |scenario, _clock| {
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
    test!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create_for_app<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        // A permit for a different app than the allowance is bound to.
        let b = alw.app_balance_spend(
            allowance::spend_permit(internal::permit<APP2>()),
            allowance::new_withdrawal_for_testing<Balance<TEST>>(id, FUNDER, 100),
            clock,
            scenario.ctx(),
        );
        b.destroy_for_testing();
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EWrongApp)]
fun test_app_spend_on_plain_allowance_rejected() {
    test!(FUNDER, |scenario, clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        let id = object::id(&alw);
        spend_as_app(&mut alw, id, 100, clock, scenario.ctx());
        ts::return_shared(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EWrongApp)]
fun test_rotate_spender_on_plain_allowance_rejected() {
    test!(FUNDER, |scenario, _clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        // Only an app-bound allowance can rotate; plain ones rotate through aliases.
        scenario.next_tx(SPENDER);
        let mut alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        alw.rotate_spender(allowance::settings_permit(internal::permit<APP>()), SPENDER2);
        ts::return_shared(alw);
    });
}

#[test]
fun test_funder_revokes() {
    test!(FUNDER, |scenario, _clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        // Issuance sent the cap to the funder; possessing it authorizes the
        // revoke.
        scenario.next_tx(FUNDER);
        let cap = scenario.take_from_sender<AllowanceCap<Balance<TEST>>>();
        let alw = scenario.take_shared<Allowance<Balance<TEST>>>();
        cap.revoke(alw);
    });
}

#[test]
#[expected_failure(abort_code = sui::allowance::EWrongCap)]
fun test_revoke_with_wrong_cap() {
    test!(FUNDER, |scenario, _clock| {
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        // Take the first allowance's cap, then issue a second allowance.
        scenario.next_tx(FUNDER);
        let cap = scenario.take_from_sender<AllowanceCap<Balance<TEST>>>();
        new_allowance().lifetime_cap(1000).create<Balance<TEST>>(scenario.ctx());

        // The first cap must not revoke the second allowance.
        scenario.next_tx(FUNDER);
        let second = ts::most_recent_id_shared<Allowance<Balance<TEST>>>().destroy_some();
        let alw = ts::take_shared_by_id<Allowance<Balance<TEST>>>(scenario, second);
        cap.revoke(alw);
    });
}
