// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Native allowances: delegated, bounded, revocable spending from an
/// address's live balance (no escrow).
///
/// The core verifies a tx's declared (funder, allowance) source at signing and
/// hands the PTB an `AllowanceWithdrawal`; the spend paths enforce policy and
/// redeem in one step, so limits are never consumed without funds moving.
module sui::allowance;

use std::string::String;
use std::type_name::{Self, TypeName};
use sui::balance::{Self, Balance};
use sui::clock::Clock;
use sui::funds_accumulator::Withdrawal;

#[error(code = 0)]
const ENotSpender: vector<u8> = b"Transaction sender is not this allowance's spender";
#[error(code = 1)]
const EWrongApp: vector<u8> = b"Permit type does not match the allowance's app";
#[error(code = 2)]
const ENoApp: vector<u8> = b"Allowance has no app, so it has no app-authorized spend or rotate";
#[error(code = 3)]
const EExpired: vector<u8> = b"Allowance has expired";
#[error(code = 4)]
const EExceedsLifetimeCap: vector<u8> = b"Spend would exceed the lifetime cap";
#[error(code = 5)]
const EExceedsRateLimit: vector<u8> = b"Spend would exceed the current rate-limit window";
#[error(code = 7)]
const ENoLimit: vector<u8> = b"Allowance must have a lifetime cap or a rate limit";
#[error(code = 8)]
const EWrongAllowance: vector<u8> = b"Withdrawal was issued for a different allowance";
#[error(code = 9)]
const EBadRateLimit: vector<u8> = b"Rate limit needs a positive period and limit";
#[error(code = 10)]
const ENotStarted: vector<u8> = b"Allowance is not active yet; it has a future start timestamp";
#[error(code = 11)]
const EHasApp: vector<u8> = b"App-bound allowance: spend through `spend_balance_as_app`";
#[error(code = 12)]
const EWrongFunder: vector<u8> =
    b"Withdrawal debits a different address than this allowance's funder";
#[error(code = 13)]
const EWrongCap: vector<u8> = b"Cap does not match this allowance";
#[error(code = 14)]
const ENameTooLong: vector<u8> = b"Name exceeds the 128-byte limit";
#[error(code = 15)]
const EZeroLifetimeCap: vector<u8> = b"Lifetime cap must be greater than zero";
#[error(code = 16)]
const EBadTimeWindow: vector<u8> = b"Expiration must be after the start time";
#[error(code = 17)]
const ENoExpiration: vector<u8> = b"Allowance must have an expiration or a rate limit";
#[error(code = 18)]
const ENotEnabled: vector<u8> = b"Allowances are not enabled";

const MAX_NAME_LENGTH: u64 = 128;
const MS_PER_DAY: u64 = 86_400_000;

/// Created by the core for a declared allowance source. Only the bound
/// allowance's spend paths can unpack it. Dropping it is fine: funds only
/// move on redemption.
public struct AllowanceWithdrawal<phantom T: store> has drop {
    allowance: ID,
    inner: Withdrawal<T>,
}

/// Delegated authority to withdraw `T` from `funder`'s balance, within limits.
/// A shared object (discoverable + revocable); the spending tx references it by id.
public struct Allowance<phantom T> has key {
    id: UID,
    funder: address,
    /// Always `Some` in the first release.
    /// `Option` so app-bound allowances can later go keyless.
    spender: Option<address>,
    /// When set, spends need the app's `Permit` on top of the spender's
    /// signature, and only the app's module can rotate the spender.
    app: Option<TypeName>,
    /// `None` = no lifetime total; at least one of cap / rate limit must be
    /// set. Amounts are `u256` (matching `Withdrawal.limit`); times are ms.
    lifetime_cap: Option<u256>,
    /// Cumulative spend, compared against `lifetime_cap`.
    current_spend: u256,
    /// Inclusive activation time; `None` = active on issue.
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_limit: Option<RateLimit>,
    /// Off-chain label, at most 128 bytes; not consulted by any check.
    name: String,
}

/// Revocation for an allowance, sent to the funder at issuance (key-only, non-transferrable).
/// Also used for discoverability (funder -> allowances)
public struct AllowanceCap<phantom T> has key {
    id: UID,
    allowance: ID,
}

/// At most `limit` per window. Windows only roll forward, so boundaries never
/// drift: `FixedWindow` periods tile absolute time from the Unix epoch, while
/// `CalendarWindow` windows follow the anniversary of the first charge.
public enum RateLimit has copy, drop, store {
    FixedWindow {
        period_ms: u64,
        limit: u256,
        spent: u256,
        window_start_ms: u64,
    },
    CalendarWindow {
        months: u8,
        limit: u256,
        spent: u256,
        /// Anniversary anchor; stamped by the first successful charge (0 until then).
        first_charge_ms: u64,
        /// Windows are numbered from the anchor (0 = first). This is the one
        /// `spent` accumulated in; a spend landing in a later one resets it.
        window: u64,
    },
}

/// App authorization for the `_as_app` endpoints. A separate type so the
/// allowance API has its own authorization type instead of `internal::Permit`.
public struct Permit<phantom A>() has drop;

/// Only `A`'s module can create `internal::Permit<A>`, so only it can build this.
public fun permit<A>(_: internal::Permit<A>): Permit<A> {
    Permit()
}

/// At most `limit` per `period_ms`, windows aligned to whole periods from the
/// Unix epoch (a 24h period resets at midnight UTC).
public fun fixed_window(period_ms: u64, limit: u256): RateLimit {
    // A zero period resets the window on every spend; a zero limit spends nothing.
    assert!(period_ms > 0 && limit > 0, EBadRateLimit);
    RateLimit::FixedWindow { period_ms, limit, spent: 0, window_start_ms: 0 }
}

/// At most `limit` per `months` civil (UTC) months, anchored at the first
/// successful charge: windows renew on its day-of-month at 00:00 UTC, clamped
/// to shorter months (a Jan 31 anchor renews Feb 28, then Mar 31).
public fun calendar_window(months: u8, limit: u256): RateLimit {
    assert!(months > 0 && limit > 0, EBadRateLimit);
    RateLimit::CalendarWindow { months, limit, spent: 0, first_charge_ms: 0, window: 0 }
}

/// At most `limit` per month, from the first charge.
public fun monthly(limit: u256): RateLimit { calendar_window(1, limit) }

/// At most `limit` per quarter, from the first charge.
public fun quarterly(limit: u256): RateLimit { calendar_window(3, limit) }

/// At most `limit` per year, from the first charge.
public fun yearly(limit: u256): RateLimit { calendar_window(12, limit) }

// `entry`, not `public`: issuance must be an explicit PTB command, so a contract
// cannot create an allowance funded by the caller inside some other call.

entry fun new<T>(
    name: String,
    spender: address,
    lifetime_cap: Option<u256>,
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_limit: Option<RateLimit>,
    ctx: &mut TxContext,
) {
    share_new<T>(
        name,
        spender,
        option::none(),
        lifetime_cap,
        start_timestamp_ms,
        expiration_timestamp_ms,
        rate_limit,
        ctx,
    );
}

/// Like `new`, but also binds the controlling app `A` (see `Allowance.app`).
entry fun new_for_app<T, A>(
    name: String,
    spender: address,
    lifetime_cap: Option<u256>,
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_limit: Option<RateLimit>,
    ctx: &mut TxContext,
) {
    share_new<T>(
        name,
        spender,
        option::some(type_name::with_defining_ids<A>()),
        lifetime_cap,
        start_timestamp_ms,
        expiration_timestamp_ms,
        rate_limit,
        ctx,
    );
}

// === Spend ===
//
// Every spend path consumes limits and redeems in one step: returning a bare
// `Withdrawal<T>` would let limits be consumed without funds actually moving.

/// Signer path: the tx sender must be the spender. (A non-balance spend would
/// require access to `funds_accumulator::Permit<T>`, so `Balance`-only for now.)
public fun spend_balance<C>(
    self: &mut Allowance<Balance<C>>,
    w: AllowanceWithdrawal<Balance<C>>,
    clock: &Clock,
    ctx: &TxContext,
): Balance<C> {
    assert!(self.app.is_none(), EHasApp);
    balance::redeem_funds(self.consume(w, clock, ctx))
}

/// App path: authorized by `Permit<A>` (matching the allowance's `app`); the
/// tx must still come from the spender.
public fun spend_balance_as_app<C, A>(
    self: &mut Allowance<Balance<C>>,
    _: Permit<A>,
    w: AllowanceWithdrawal<Balance<C>>,
    clock: &Clock,
    ctx: &TxContext,
): Balance<C> {
    self.assert_app<Balance<C>, A>();
    balance::redeem_funds(self.consume(w, clock, ctx))
}

/// Possession of the matching cap is what authorizes revocation; no signer check.
public fun revoke<T>(self: Allowance<T>, cap: AllowanceCap<T>) {
    let AllowanceCap { id: cap_id, allowance } = cap;
    assert!(allowance == self.id.to_inner(), EWrongCap);
    let Allowance {
        id,
        ..,
    } = self;
    id.delete();
    cap_id.delete();
}

/// App-only: rotate the spender key without the funder reissuing.
public fun rotate_spender<T, A>(self: &mut Allowance<T>, _: Permit<A>, new_spender: address) {
    self.assert_app<T, A>();
    self.spender = option::some(new_spender);
}

// TODO: update endpoints for altering limits, expiration, etc.

fun assert_app<T, A>(self: &Allowance<T>) {
    assert!(self.app.is_some(), ENoApp);
    assert!(*self.app.borrow() == type_name::with_defining_ids<A>(), EWrongApp);
}

/// Policy checks + accounting shared by all spend paths. The spender gate
/// lives here; the app / no-app split stays with the callers.
fun consume<T: store>(
    self: &mut Allowance<T>,
    w: AllowanceWithdrawal<T>,
    clock: &Clock,
    ctx: &TxContext,
): Withdrawal<T> {
    let AllowanceWithdrawal { allowance, inner } = w;
    assert!(allowance == self.id.to_inner(), EWrongAllowance);
    assert!(self.spender.contains(&ctx.sender()), ENotSpender);
    // Signing already verified the funder; defense in depth against a core bug.
    assert!(inner.owner() == self.funder, EWrongFunder);
    let amount = inner.limit();
    let now = clock.timestamp_ms();

    self.start_timestamp_ms.do_ref!(|start_timestamp_ms| {
        assert!(now >= *start_timestamp_ms, ENotStarted);
    });

    self.expiration_timestamp_ms.do_ref!(|expiration_timestamp_ms| {
        assert!(now <= *expiration_timestamp_ms, EExpired);
    });

    self.lifetime_cap.do_ref!(|lifetime_cap| {
        assert!(self.current_spend + amount <= *lifetime_cap, EExceedsLifetimeCap);
    });

    self.current_spend = self.current_spend + amount;

    self
        .rate_limit
        .do_mut!(
            |rl| match (rl) {
                RateLimit::FixedWindow { period_ms, limit, spent, window_start_ms } => {
                    // Roll forward by whole periods only, keeping the epoch grid.
                    if (now - *window_start_ms >= *period_ms) {
                        let elapsed_periods = (now - *window_start_ms) / *period_ms;
                        *window_start_ms = *window_start_ms + elapsed_periods * *period_ms;
                        *spent = 0;
                    };
                    assert!(*spent + amount <= *limit, EExceedsRateLimit);
                    *spent = *spent + amount;
                },
                RateLimit::CalendarWindow { months, limit, spent, first_charge_ms, window } => {
                    if (*first_charge_ms == 0) {
                        // The first successful charge anchors the windows; an
                        // aborted spend unwinds the stamp.
                        *first_charge_ms = now;
                    } else {
                        let k = elapsed_windows(*first_charge_ms, now, *months);
                        if (k > *window) {
                            *window = k;
                            *spent = 0;
                        };
                    };
                    assert!(*spent + amount <= *limit, EExceedsRateLimit);
                    *spent = *spent + amount;
                },
            },
        );

    inner
}

/// How many whole `months`-month windows have elapsed since `anchor_ms` —
/// equivalently, the number of the window `now_ms` falls in (0 = first).
fun elapsed_windows(anchor_ms: u64, now_ms: u64, months: u8): u64 {
    let (anchor_year, anchor_month, anchor_day) = civil_from_ms(anchor_ms);
    let (year, month, day) = civil_from_ms(now_ms);
    let mut elapsed = (year * 12 + month) - (anchor_year * 12 + anchor_month);
    // A month only fully elapses once the anniversary day arrives, clamped to
    // month ends (a Jan 31 anchor renews Feb 28). Same-month spends have
    // day >= anchor_day already; the elapsed > 0 guard is underflow safety.
    if (elapsed > 0 && day < anchor_day.min(days_in_month(year, month))) {
        elapsed = elapsed - 1;
    };
    elapsed / (months as u64)
}

/// Civil (year, month, day) in UTC — month and day 1-based, so the epoch is
/// `(1970, 1, 1)` — via Hinnant's `civil_from_days`; unsigned-only works
/// because chain timestamps are never pre-epoch.
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

public(package) fun days_in_month(year: u64, month: u64): u64 {
    if (month == 2) {
        if (is_leap_year(year)) 29 else 28
    } else if (month == 4 || month == 6 || month == 9 || month == 11) 30
    else 31
}

/// Gregorian rule: every 4th year, except centuries not divisible by 400.
fun is_leap_year(year: u64): bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fun share_new<T>(
    name: String,
    spender: address,
    app: Option<TypeName>,
    lifetime_cap: Option<u256>,
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_limit: Option<RateLimit>,
    ctx: &mut TxContext,
) {
    assert!(sui::protocol_config::is_feature_enabled(b"enable_allowances"), ENotEnabled);
    // we do not allow unlimited allowances (TODO: Do we?)
    assert!(lifetime_cap.is_some() || rate_limit.is_some(), ENoLimit);
    assert!(expiration_timestamp_ms.is_some() || rate_limit.is_some(), ENoExpiration);
    assert!(name.length() <= MAX_NAME_LENGTH, ENameTooLong);
    lifetime_cap.do_ref!(|cap| assert!(*cap > 0, EZeroLifetimeCap));

    if (start_timestamp_ms.is_some() && expiration_timestamp_ms.is_some()) {
        assert!(*start_timestamp_ms.borrow() < *expiration_timestamp_ms.borrow(), EBadTimeWindow);
    };

    let allowance = Allowance<T> {
        id: object::new(ctx),
        name,
        funder: ctx.sender(),
        spender: option::some(spender),
        app,
        lifetime_cap,
        current_spend: 0,
        start_timestamp_ms,
        expiration_timestamp_ms,
        rate_limit,
    };
    let cap = AllowanceCap<T> {
        id: object::new(ctx),
        allowance: allowance.id.to_inner(),
    };
    transfer::transfer(cap, ctx.sender());
    transfer::share_object(allowance);
}

// === Test-only ===

/// Stands in for the core-issued reservation; the protocol creates these,
/// Move code cannot.
#[test_only]
public fun new_withdrawal_for_testing<T: store>(
    allowance: ID,
    funder: address,
    amount: u256,
): AllowanceWithdrawal<T> {
    AllowanceWithdrawal {
        allowance,
        inner: sui::funds_accumulator::create_withdrawal<T>(funder, amount),
    }
}
