// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// SAMPLE / API SKETCH: native allowances. Delegated, bounded, revocable
/// spending from an address's live balance (no escrow).
///
/// The core verifies a tx's declared (funder, allowance) source at signing and
/// hands the PTB an `AllowanceWithdrawal`; the spend paths enforce policy and
/// redeem in one step, so limits are never consumed without funds moving.
module sui::allowance;

use std::type_name::{Self, TypeName};
use sui::balance::{Self, Balance};
use sui::clock::Clock;
use sui::funds_accumulator::Withdrawal;

// === Errors ===

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
#[error(code = 6)]
const ENotFunder: vector<u8> = b"Only the funder may update or revoke this allowance";
#[error(code = 7)]
const ENoLimit: vector<u8> = b"Allowance must have a lifetime cap or a rate limit";
#[error(code = 8)]
const EWrongAllowance: vector<u8> = b"Withdrawal was issued for a different allowance";
#[error(code = 9)]
const EBadRateLimit: vector<u8> = b"Rate limit needs both a period and an amount, or neither";
#[error(code = 10)]
const ENotStarted: vector<u8> = b"Allowance is not active yet; tt has a future start timestamp.";
#[error(code = 11)]
const EHasApp: vector<u8> = b"App-controlled allowance: spending must go through `spend_as_app`";
#[error(code = 12)]
const EWrongFunder: vector<u8> =
    b"Withdrawal debits a different address than this allowance's funder";

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
    /// Always `Some` today: the sign-time DoS gate needs a concrete signer.
    /// `Option` so app-bound allowances can later go keyless.
    spender: Option<address>,
    /// When set, only the app's module can spend and rotate; the signer path
    /// is disabled and `spender` is just the sign-time gate.
    app: Option<TypeName>,
    /// `None` = no lifetime total; at least one of cap / rate limit must be
    /// set. Amounts are `u256` (matching `Withdrawal.limit`); times are ms.
    lifetime_cap: Option<u256>,
    current_spend: u256,
    /// Inclusive activation time; `None` = active on issue.
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_limit: Option<RateLimit>,
}

/// A tumbling per-window cap: at most `limit` per `period_ms`, resetting each
/// window.
public struct RateLimit has copy, drop, store {
    period_ms: u64,
    limit: u256,
    spent: u256,
    window_start_ms: u64,
}

/// App authorization for the `_as_app` endpoints. A separate type so the
/// allowance API has its own authorization type instead of `internal::Permit`.
public struct Permit<phantom A>() has drop;

/// Only `A`'s module can create `internal::Permit<A>`, so only it can build this.
public fun permit<A>(_: internal::Permit<A>): Permit<A> {
    Permit()
}

// === Issuance ===
//
// `entry`, not `public`: issuance must be an explicit PTB command, so a contract
// cannot create an allowance funded by the caller inside some other call.

/// Issue a signer-only allowance funded by the sender, delegating to `spender`.
entry fun new<T>(
    spender: address,
    lifetime_cap: Option<u256>,
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_period_ms: Option<u64>,
    rate_amount: Option<u256>,
    ctx: &mut TxContext,
) {
    share_new<T>(
        spender,
        option::none(),
        lifetime_cap,
        start_timestamp_ms,
        expiration_timestamp_ms,
        build_rate_limit(rate_period_ms, rate_amount),
        ctx,
    );
}

/// Like `new`, but also binds the controlling app `A` (see `Allowance.app`).
entry fun new_for_app<T, A>(
    spender: address,
    lifetime_cap: Option<u256>,
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_period_ms: Option<u64>,
    rate_amount: Option<u256>,
    ctx: &mut TxContext,
) {
    share_new<T>(
        spender,
        option::some(type_name::with_defining_ids<A>()),
        lifetime_cap,
        start_timestamp_ms,
        expiration_timestamp_ms,
        build_rate_limit(rate_period_ms, rate_amount),
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
    self.assert_signer(ctx);
    balance::redeem_funds(self.consume(w, clock))
}

/// App path: authorized by `Permit<A>` (matching the allowance's `app`), no
/// signer required.
public fun spend_balance_as_app<C, A>(
    self: &mut Allowance<Balance<C>>,
    _: Permit<A>,
    w: AllowanceWithdrawal<Balance<C>>,
    clock: &Clock,
): Balance<C> {
    self.assert_app<Balance<C>, A>();
    balance::redeem_funds(self.consume(w, clock))
}

public fun revoke<T>(self: Allowance<T>, ctx: &TxContext) {
    assert!(self.funder == ctx.sender(), ENotFunder);
    let Allowance {
        id,
        ..,
    } = self;
    id.delete();
}

/// App-only: rotate the spender key without the funder reissuing.
public fun rotate_spender<T, A>(self: &mut Allowance<T>, _: Permit<A>, new_spender: address) {
    self.assert_app<T, A>();
    self.spender = option::some(new_spender);
}

/// Signer path: no controlling app, and the tx sender is the spender.
fun assert_signer<T>(self: &Allowance<T>, ctx: &TxContext) {
    assert!(self.app.is_none(), EHasApp);
    assert!(self.spender.contains(&ctx.sender()), ENotSpender);
}

/// App-path authorization: `A` matches the allowance's controlling app.
fun assert_app<T, A>(self: &Allowance<T>) {
    assert!(self.app.is_some(), ENoApp);
    assert!(*self.app.borrow() == type_name::with_defining_ids<A>(), EWrongApp);
}

/// Policy checks + accounting shared by all spend paths; authorization is the
/// callers' responsibility.
fun consume<T: store>(
    self: &mut Allowance<T>,
    w: AllowanceWithdrawal<T>,
    clock: &Clock,
): Withdrawal<T> {
    let AllowanceWithdrawal { allowance, inner } = w;
    assert!(allowance == self.id.to_inner(), EWrongAllowance);
    // This can only happen if we have a bug in the core, so this is just for in-depth defense.
    assert!(inner.owner() == self.funder, EWrongFunder);
    let amount = inner.limit();

    let now = clock.timestamp_ms();
    if (self.start_timestamp_ms.is_some()) {
        assert!(now >= *self.start_timestamp_ms.borrow(), ENotStarted);
    };
    if (self.expiration_timestamp_ms.is_some()) {
        assert!(now <= *self.expiration_timestamp_ms.borrow(), EExpired);
    };

    if (self.lifetime_cap.is_some()) {
        assert!(self.current_spend + amount <= *self.lifetime_cap.borrow(), EExceedsLifetimeCap);
    };
    self.current_spend = self.current_spend + amount;

    if (self.rate_limit.is_some()) {
        let rl = self.rate_limit.borrow_mut();
        // Tumbling window: reset once the period has elapsed.
        if (now >= rl.window_start_ms + rl.period_ms) {
            rl.window_start_ms = now;
            rl.spent = 0;
        };
        assert!(rl.spent + amount <= rl.limit, EExceedsRateLimit);
        rl.spent = rl.spent + amount;
    };

    inner
}

/// Both `Some` (a limit) or both `None` (no limit); a mismatch aborts.
fun build_rate_limit(period_ms: Option<u64>, amount: Option<u256>): Option<RateLimit> {
    assert!(period_ms.is_some() == amount.is_some(), EBadRateLimit);
    if (period_ms.is_none()) option::none()
    else
        option::some(RateLimit {
            period_ms: *period_ms.borrow(),
            limit: *amount.borrow(),
            spent: 0,
            window_start_ms: 0,
        })
}

fun share_new<T>(
    spender: address,
    app: Option<TypeName>,
    lifetime_cap: Option<u256>,
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_limit: Option<RateLimit>,
    ctx: &mut TxContext,
) {
    // we do not allow unlimited allowances (TODO: Do we want to?)
    assert!(lifetime_cap.is_some() || rate_limit.is_some(), ENoLimit);
    let allowance = Allowance<T> {
        id: object::new(ctx),
        funder: ctx.sender(),
        spender: option::some(spender),
        app,
        lifetime_cap,
        current_spend: 0,
        start_timestamp_ms,
        expiration_timestamp_ms,
        rate_limit,
    };
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

