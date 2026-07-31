// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Native allowances enable delegated, bounded, revocable spending from an address's live balance.
///
/// A transaction declares its funding source as a (funder, allowance) pair. Signing verifies that
/// source against the shared `Allowance` and hands the transaction an `AllowanceWithdrawal`.
///
/// All policy checks (rate limits, lifetime caps, expiry) are enforced by this module.
module sui::allowance;

use std::string::String;
use std::type_name::{Self, TypeName};
use sui::balance::{Self, Balance};
use sui::clock::Clock;
use sui::funds_accumulator::Withdrawal;

public use fun allowance_settings as Allowance.settings;
public use fun allowance_current_spend as Allowance.current_spend;
public use fun allowance_cap_allowance as AllowanceCap.allowance;
public use fun allowance_proposal_settings as AllowanceProposal.settings;
public use fun rate_limit_limit as RateLimit.limit;
public use fun rate_limit_spent as RateLimit.spent;
public use fun rate_limit_window as RateLimit.window;

#[error(code = 0)]
const ENotSpender: vector<u8> = "Transaction sender is not this allowance's spender";
#[error(code = 1)]
const EWrongApp: vector<u8> = "Allowance is not bound to this app";
#[error(code = 2)]
const EExpired: vector<u8> = "Allowance has expired";
#[error(code = 3)]
const EExceedsLifetimeCap: vector<u8> = "Spend would exceed the lifetime cap";
#[error(code = 4)]
const EExceedsRateLimit: vector<u8> = "Spend would exceed the current rate-limit window";
#[error(code = 5)]
const ENoLimit: vector<u8> = "Allowance must have a lifetime cap or a rate limit";
#[error(code = 6)]
const EWrongAllowance: vector<u8> = "Withdrawal was issued for a different allowance";
#[error(code = 7)]
const EBadRateLimit: vector<u8> = "Rate limit needs a positive period and limit";
#[error(code = 8)]
const ENotStarted: vector<u8> = "Allowance is not active yet; it has a future start timestamp";
#[error(code = 9)]
const EHasApp: vector<u8> = "App-bound allowance: spend through `app_balance_spend`";
#[error(code = 10)]
const EWrongFunder: vector<u8> =
    "Withdrawal debits a different address than this allowance's funder";
#[error(code = 11)]
const EWrongCap: vector<u8> = "Cap does not match this allowance";
#[error(code = 12)]
const ENameTooLong: vector<u8> = "Name exceeds the 128-byte limit";
#[error(code = 13)]
const EZeroLifetimeCap: vector<u8> = "Lifetime cap must be greater than zero";
#[error(code = 14)]
const EBadTimeWindow: vector<u8> = "Expiration must be after the start time";
#[error(code = 15)]
const ENoExpiration: vector<u8> = "Allowance must have an expiration or a rate limit";
#[error(code = 16)]
const ENotEnabled: vector<u8> = "Allowances are not enabled";
#[error(code = 17)]
const ESponsorWithdrawalNotEnabled: vector<u8> = "Sponsor allowance withdrawals are not enabled";

const MAX_NAME_LENGTH: u64 = 128;
const MS_PER_DAY: u64 = 86_400_000;

/// Created via a PTB Argument.
///
/// The inner `Withdrawal` is contained within this module and cannot be accessed directly.
/// An allowance's limits can never be charged without the funds actually moving.
public struct AllowanceWithdrawal<phantom T: store> has drop {
    allowance: ID,
    /// Today is always false. Opens the door for future `ctx.sponsor()` based allowances.
    is_sponsor: bool,
    inner: Withdrawal<T>,
}

/// Enables withdrawing `T` from the funder's balance, within this allowance's limits.
/// Always kept as a shared object.
public struct Allowance<phantom T> has key {
    id: UID,
    settings: Settings,
    /// Total cumulative spend from this allowance.
    current_spend: u256,
}

/// Configuration of the Allowance, held by `Allowance` and `AllowanceProposal`
public struct Settings has drop, store {
    /// The address whose balance is debited by spends against this allowance.
    funder: address,
    /// The spender of the allowance.
    /// While it is currently always set, in the future this may become optional
    /// to allow for keyless app-bound withdrawals.
    spender: Option<address>,
    /// When set, requires the app's `SpendPermit` to spend, and only that app
    /// can rotate the spender or issue the allowance in the first place.
    app: Option<TypeName>,
    /// An optional lifetime cap on withdrawals using this allowance. Inclusive.
    /// Amounts are `u256`, matching `Withdrawal.limit`.
    lifetime_cap: Option<u256>,
    /// Optional activation time, in milliseconds. Inclusive.
    start_timestamp_ms: Option<u64>,
    /// Optional expiration time, in milliseconds. Exclusive.
    expiration_timestamp_ms: Option<u64>,
    /// An optional recurring limit, applied on top of `lifetime_cap`. At least one of the two
    /// must be set.
    rate_limit: Option<RateLimit>,
    /// A label for off-chain use, never read by any check. At most 128 bytes.
    name: String,
}

/// Revocation for an allowance, sent to the funder at issuance (soulbound).
/// Also used for discoverability (funder -> allowances).
///
/// Created with the allowance and destroyed with it by `revoke`.
public struct AllowanceCap<phantom T> has key {
    id: UID,
    allowance: ID,
}

/// A recurring cap on withdrawals, applied on top of any lifetime cap.
///
/// An enum so other mechanics, like sliding windows, can be added as variants.
public enum RateLimit has copy, drop, store {
    /// At most `limit` per window. Windows only roll forward.
    Windowed {
        /// The most that can be spent within a window. Inclusive.
        limit: u256,
        /// Amount spent so far within the current window.
        spent: u256,
        /// Start of the first window, stamped by the first successful charge.
        anchor_ms: Option<u64>,
        /// Which window `spent` is accumulated in, numbered from the anchor (0 = first).
        /// A spend landing in a later one resets `spent`.
        index: u64,
        /// The defining period of time for this rate limit.
        window: Window,
    },
}

/// The defining time period for a `RateLimit::Windowed`.
public enum Window has copy, drop, store {
    /// Windows of exactly this many milliseconds.
    PeriodicMs(u64),
    /// Windows of this many civil (UTC) months.
    CalendarMonths(u8),
}

/// A proposal that can only be issued by app `A`.
public struct AllowanceProposal<phantom T>(Settings) has drop;

/// A `SpendPermit<A>` authorizes a single spend against an allowance bound to
/// `A`. It is issued from an `internal::Permit<A>`, allowing the module that
/// defines `A` to gate every withdrawal on its own logic.
public struct SpendPermit<phantom A>() has drop;

/// A `SettingsPermit<A>` authorizes changing the configuration of an allowance bound to `A`:
/// issuing one, or rotating its spender. It is issued from an `internal::Permit<A>`.
///
/// Kept distinct from `SpendPermit` so an app can hand out the right to spend without also
/// handing out the right to reconfigure, and vice versa.
public struct SettingsPermit<phantom A>() has drop;

/// Issues a `SpendPermit<A>` from the privileged `internal::Permit<A>`.
public fun spend_permit<A>(_: internal::Permit<A>): SpendPermit<A> {
    SpendPermit()
}

/// Issues a `SettingsPermit<A>` from the privileged `internal::Permit<A>`.
public fun settings_permit<A>(_: internal::Permit<A>): SettingsPermit<A> {
    SettingsPermit()
}

// === Rate limits ===

/// At most `limit` per `period_ms`, counted from the first charge.
public fun periodic_rate_limit(period_ms: u64, limit: u256): RateLimit {
    // A zero period resets the window on every spend; a zero limit spends nothing.
    assert!(period_ms > 0 && limit > 0, EBadRateLimit);
    new_rate_limit(limit, Window::PeriodicMs(period_ms))
}

/// At most `limit` per `months` civil (UTC) months, counted from the first charge. Windows renew
/// on the anchor's day-of-month at 00:00 UTC, clamped to shorter months (a Jan 31 anchor renews
/// Feb 28, then Mar 31).
public fun calendar_rate_limit(months: u8, limit: u256): RateLimit {
    assert!(months > 0 && limit > 0, EBadRateLimit);
    new_rate_limit(limit, Window::CalendarMonths(months))
}

public fun monthly_rate_limit(limit: u256): RateLimit { calendar_rate_limit(1, limit) }

public fun quarterly_rate_limit(limit: u256): RateLimit { calendar_rate_limit(3, limit) }

public fun yearly_rate_limit(limit: u256): RateLimit { calendar_rate_limit(12, limit) }

// === Issuance ===

/// Issues an allowance funded by the sender, sharing it and sending its `AllowanceCap` to the
/// sender. Creation is `entry` so contracts cannot create allowances implicitly.
entry fun new<T>(
    name: String,
    spender: address,
    lifetime_cap: Option<u256>,
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_limit: Option<RateLimit>,
    ctx: &mut TxContext,
) {
    let settings = new_settings(
        ctx.sender(),
        spender,
        option::none(),
        lifetime_cap,
        start_timestamp_ms,
        expiration_timestamp_ms,
        rate_limit,
        name,
    );
    share_new<T>(settings, ctx);
}

/// Returns an `AllowanceProposal` for an allowance bound to the controlling app `A`, funded by
/// the sender.
///
/// Unlike `new`, this creates no allowance on its own. `A`'s module must accept the proposal via
/// `issue`, giving the app a say in every allowance that names it.
entry fun propose_for_app<T, A>(
    name: String,
    spender: address,
    lifetime_cap: Option<u256>,
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_limit: Option<RateLimit>,
    ctx: &TxContext,
): AllowanceProposal<T> {
    let settings = new_settings(
        ctx.sender(),
        spender,
        option::some(type_name::with_defining_ids<A>()),
        lifetime_cap,
        start_timestamp_ms,
        expiration_timestamp_ms,
        rate_limit,
        name,
    );
    AllowanceProposal(settings)
}

/// Issues the proposed allowance on `A`'s behalf, creating and sharing it.
public fun issue<T, A>(proposal: AllowanceProposal<T>, _: SettingsPermit<A>, ctx: &mut TxContext) {
    let AllowanceProposal(settings) = proposal;
    assert!(settings.app.contains(&type_name::with_defining_ids<A>()), EWrongApp);
    share_new<T>(settings, ctx);
}

// === Spend ===

/// Signer path: the tx sender must be the spender.
public fun balance_spend<C>(
    self: &mut Allowance<Balance<C>>,
    w: AllowanceWithdrawal<Balance<C>>,
    clock: &Clock,
    ctx: &TxContext,
): Balance<C> {
    assert!(self.settings.app.is_none(), EHasApp);
    balance::redeem_funds(self.consume(w, clock, ctx))
}

/// App path: requires a `SpendPermit<A>` matching the allowance's `app`. The tx must still come
/// from the spender.
public fun app_balance_spend<C, A>(
    self: &mut Allowance<Balance<C>>,
    _: SpendPermit<A>,
    w: AllowanceWithdrawal<Balance<C>>,
    clock: &Clock,
    ctx: &TxContext,
): Balance<C> {
    self.assert_app<Balance<C>, A>();
    balance::redeem_funds(self.consume(w, clock, ctx))
}

/// Revokes an allowance, removing the ability to spend.
public fun revoke<T>(self: AllowanceCap<T>, allowance: Allowance<T>) {
    let AllowanceCap { id: cap_id, allowance: cap_allowance } = self;
    assert!(cap_allowance == allowance.id.to_inner(), EWrongCap);
    let Allowance { id, .. } = allowance;
    id.delete();
    cap_id.delete();
}

/// Rotates the spender key without the funder reissuing the allowance.
///
/// App-only: for an app-bound allowance the app dictates who the spender is. Non-app allowances
/// rotate through address aliases instead.
public fun rotate_spender<T, A>(
    self: &mut Allowance<T>,
    _: SettingsPermit<A>,
    new_spender: address,
) {
    self.assert_app<T, A>();
    self.settings.spender = option::some(new_spender);
}

// === Accessors ===

public fun allowance_settings<T>(self: &Allowance<T>): &Settings { &self.settings }

public fun allowance_current_spend<T>(self: &Allowance<T>): u256 { self.current_spend }

public fun allowance_cap_allowance<T>(self: &AllowanceCap<T>): ID { self.allowance }

public fun allowance_proposal_settings<T>(self: &AllowanceProposal<T>): &Settings { &self.0 }

public fun funder(self: &Settings): address { self.funder }

public fun spender(self: &Settings): Option<address> { self.spender }

public fun app(self: &Settings): Option<TypeName> { self.app }

public fun lifetime_cap(self: &Settings): Option<u256> { self.lifetime_cap }

public fun start_timestamp_ms(self: &Settings): Option<u64> { self.start_timestamp_ms }

public fun expiration_timestamp_ms(self: &Settings): Option<u64> {
    self.expiration_timestamp_ms
}

public fun rate_limit(self: &Settings): Option<RateLimit> { self.rate_limit }

public fun name(self: &Settings): &String { &self.name }

public fun rate_limit_limit(self: &RateLimit): u256 {
    match (self) {
        RateLimit::Windowed { limit, .. } => *limit,
    }
}

public fun rate_limit_spent(self: &RateLimit): u256 {
    match (self) {
        RateLimit::Windowed { spent, .. } => *spent,
    }
}

public fun rate_limit_window(self: &RateLimit): Window {
    match (self) {
        RateLimit::Windowed { window, .. } => *window,
    }
}

// === Package ===

/// Civil (year, month, day) in UTC, month and day 1-based. This is Howard Hinnant's
/// `civil_from_days`, where the derivation of every constant here is documented:
/// https://howardhinnant.github.io/date_algorithms.html#civil_from_days
///
/// The unsigned-only form of the algorithm. Chain timestamps are never pre-epoch.
public(package) fun civil_from_ms(timestamp_ms: u64): (u64, u64, u64) {
    let z = timestamp_ms / MS_PER_DAY + 719468;
    let era = z / 146097;
    let doe = z % 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let (year, month) = if (mp < 10) (era * 400 + yoe, mp + 3) else (era * 400 + yoe + 1, mp - 9);
    (year, month, day)
}

/// Companion to `civil_from_ms`, following the same reference:
/// https://howardhinnant.github.io/date_algorithms.html#last_day_of_month
public(package) fun days_in_month(year: u64, month: u64): u64 {
    match (month) {
        2 if (is_leap_year(year)) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// === Internals ===

fun assert_app<T, A>(self: &Allowance<T>) {
    let app = type_name::with_defining_ids<A>();
    assert!(self.settings.app.contains(&app), EWrongApp);
}

/// Central logic for policy checks and accounting, including the check of the
/// spender. NB: Any app checks must be done beforehand by the caller.
fun consume<T: store>(
    self: &mut Allowance<T>,
    w: AllowanceWithdrawal<T>,
    clock: &Clock,
    ctx: &TxContext,
): Withdrawal<T> {
    let AllowanceWithdrawal { allowance, is_sponsor, inner } = w;
    assert!(!is_sponsor, ESponsorWithdrawalNotEnabled);
    assert!(allowance == self.id.to_inner(), EWrongAllowance);
    assert!(self.settings.spender.contains(&ctx.sender()), ENotSpender);
    // Defense-in-depth check as this should already be verified at signing.
    assert!(inner.owner() == self.settings.funder, EWrongFunder);
    let amount = inner.limit();
    let now = clock.timestamp_ms();

    self.settings.start_timestamp_ms.do_ref!(|start_timestamp_ms| {
        assert!(now >= *start_timestamp_ms, ENotStarted);
    });

    self.settings.expiration_timestamp_ms.do_ref!(|expiration_timestamp_ms| {
        assert!(now < *expiration_timestamp_ms, EExpired);
    });

    self.current_spend = self.current_spend + amount;
    self.settings.lifetime_cap.do_ref!(|lifetime_cap| {
        assert!(self.current_spend <= *lifetime_cap, EExceedsLifetimeCap);
    });

    self.settings.rate_limit.do_mut!(|rate_limit| rate_limit.charge(amount, now));

    inner
}

fun new_rate_limit(limit: u256, window: Window): RateLimit {
    RateLimit::Windowed { limit, spent: 0, anchor_ms: option::none(), index: 0, window }
}

/// Records `amount` against the limit, aborting if it does not fit.
fun charge(self: &mut RateLimit, amount: u256, now_ms: u64) {
    match (self) {
        RateLimit::Windowed { limit, spent, anchor_ms, index, window } => {
            if (anchor_ms.is_some()) {
                let current = window.index_at(*anchor_ms.borrow(), now_ms);
                if (current > *index) {
                    *index = current;
                    *spent = 0;
                };
            } else {
                // The first successful charge anchors the windows.
                *anchor_ms = option::some(now_ms);
            };
            *spent = *spent + amount;
            assert!(*spent <= *limit, EExceedsRateLimit);
        },
    }
}

/// Which window `now_ms` falls in, numbered from `anchor_ms` (0 = first).
fun index_at(self: &Window, anchor_ms: u64, now_ms: u64): u64 {
    match (self) {
        Window::PeriodicMs(period_ms) => (now_ms - anchor_ms) / *period_ms,
        Window::CalendarMonths(months) => elapsed_windows(anchor_ms, now_ms, *months),
    }
}

/// Which `months`-month window `now_ms` falls in, counting from `anchor_ms`
/// (0 = the window the anchor itself is in).
fun elapsed_windows(anchor_ms: u64, now_ms: u64, months: u8): u64 {
    let (anchor_year, anchor_month, anchor_day) = civil_from_ms(anchor_ms);
    let (year, month, day) = civil_from_ms(now_ms);
    let mut elapsed = (year * 12 + month) - (anchor_year * 12 + anchor_month);
    // A month only fully elapses once the anniversary day arrives, clamped to month ends.
    if (elapsed > 0 && day < anchor_day.min(days_in_month(year, month))) {
        elapsed = elapsed - 1;
    };
    elapsed / (months as u64)
}

fun is_leap_year(year: u64): bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Builds and validates settings. Rejects allowances that are unbounded in amount or in time.
fun new_settings(
    funder: address,
    spender: address,
    app: Option<TypeName>,
    lifetime_cap: Option<u256>,
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_limit: Option<RateLimit>,
    name: String,
): Settings {
    assert!(sui::protocol_config::is_feature_enabled("enable_allowances"), ENotEnabled);
    assert!(lifetime_cap.is_some() || rate_limit.is_some(), ENoLimit);
    assert!(expiration_timestamp_ms.is_some() || rate_limit.is_some(), ENoExpiration);
    assert!(name.length() <= MAX_NAME_LENGTH, ENameTooLong);
    lifetime_cap.do_ref!(|cap| assert!(*cap > 0, EZeroLifetimeCap));

    if (start_timestamp_ms.is_some() && expiration_timestamp_ms.is_some()) {
        assert!(*start_timestamp_ms.borrow() < *expiration_timestamp_ms.borrow(), EBadTimeWindow);
    };

    Settings {
        funder,
        spender: option::some(spender),
        app,
        lifetime_cap,
        start_timestamp_ms,
        expiration_timestamp_ms,
        rate_limit,
        name,
    }
}

fun share_new<T>(settings: Settings, ctx: &mut TxContext) {
    let funder = settings.funder;
    let allowance = Allowance<T> {
        id: object::new(ctx),
        settings,
        current_spend: 0,
    };
    let cap = AllowanceCap<T> {
        id: object::new(ctx),
        allowance: allowance.id.to_inner(),
    };
    transfer::transfer(cap, funder);
    transfer::share_object(allowance);
}

// === Test-only ===

/// Test stand-in for the withdrawal that the protocol mints from a transaction's
/// allowance-backed withdrawal input. Outside of tests, these only enter Move as PTB inputs.
#[test_only]
public fun new_withdrawal_for_testing<T: store>(
    allowance: ID,
    funder: address,
    amount: u256,
): AllowanceWithdrawal<T> {
    AllowanceWithdrawal {
        allowance,
        is_sponsor: false,
        inner: sui::funds_accumulator::create_withdrawal<T>(funder, amount),
    }
}

/// Sponsor-bound variant; no protocol path mints these yet.
#[test_only]
public fun new_sponsor_withdrawal_for_testing<T: store>(
    allowance: ID,
    funder: address,
    amount: u256,
): AllowanceWithdrawal<T> {
    AllowanceWithdrawal {
        allowance,
        is_sponsor: true,
        inner: sui::funds_accumulator::create_withdrawal<T>(funder, amount),
    }
}
