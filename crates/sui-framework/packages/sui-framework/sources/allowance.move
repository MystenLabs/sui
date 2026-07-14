// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// SAMPLE / API SKETCH: native allowances. Delegated, bounded, revocable
/// spending from an address's live balance (no escrow).
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
const EBadRateLimit: vector<u8> =
    b"Rate limit needs a positive period and amount, both set or neither";
#[error(code = 10)]
const ENotStarted: vector<u8> = b"Allowance is not active yet; it has a future start timestamp";
#[error(code = 11)]
const EHasApp: vector<u8> = b"App-controlled allowance: spending must go through `spend_as_app`";
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

const MAX_NAME_LENGTH: u64 = 128;

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
    /// When set, only the app's module can spend and rotate; the signer path
    /// is disabled and `spender` is just the sign-time gate.
    app: Option<TypeName>,
    /// `None` = no lifetime total; at least one of cap / rate limit must be
    /// set. Amounts are `u256` (matching `Withdrawal.limit`); times are ms.
    lifetime_cap: Option<u256>,
    /// The total spend, to date, of this allowance. Gets bumped on every spend.
    current_spend: u256,
    /// Inclusive activation time; `None` = active on issue.
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_limit: Option<RateLimit>,
    /// custom label, at most 128 bytes; only present for off-chain consumption
    /// (adding a short desc or label for an allowance, not consulted by any check)
    name: String,
}

/// Revocation for an allowance, sent to the funder at issuance (key-only, non-transferrable).
/// Also used for discoverability (funder -> allowances)
public struct AllowanceCap<phantom T> has key {
    id: UID,
    allowance: ID,
}

/// A tumbling cap: at most `limit` per `period_ms`, the window restarting at
/// the first spend after it elapses. An enum to leave layout room for future
/// kinds (public because the compiler does not support internal enums yet).
public enum RateLimit has copy, drop, store {
    FixedWindow {
        period_ms: u64,
        limit: u256,
        spent: u256,
        window_start_ms: u64,
    },
}

/// App authorization for the `_as_app` endpoints. A separate type so the
/// allowance API has its own authorization type instead of `internal::Permit`.
public struct Permit<phantom A>() has drop;

/// Only `A`'s module can create `internal::Permit<A>`, so only it can build this.
public fun permit<A>(_: internal::Permit<A>): Permit<A> {
    Permit()
}

// `entry`, not `public`: issuance must be an explicit PTB command, so a contract
// cannot create an allowance funded by the caller inside some other call.

entry fun new<T>(
    name: String,
    spender: address,
    lifetime_cap: Option<u256>,
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_period_ms: Option<u64>,
    rate_amount: Option<u256>,
    ctx: &mut TxContext,
) {
    share_new<T>(
        name,
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
    name: String,
    spender: address,
    lifetime_cap: Option<u256>,
    start_timestamp_ms: Option<u64>,
    expiration_timestamp_ms: Option<u64>,
    rate_period_ms: Option<u64>,
    rate_amount: Option<u256>,
    ctx: &mut TxContext,
) {
    share_new<T>(
        name,
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

// TODO: Add update endpoints to be able to alter limits, expirations etc.

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

    self.rate_limit.do_mut!(|rl| match (rl) {
        RateLimit::FixedWindow { period_ms, limit, spent, window_start_ms } => {
            // Tumbling window: reset once the period has elapsed.
            if (now >= *window_start_ms + *period_ms) {
                *window_start_ms = now;
                *spent = 0;
            };
            assert!(*spent + amount <= *limit, EExceedsRateLimit);
            *spent = *spent + amount;
        },
    });

    inner
}

/// Both `Some` (a limit) or both `None` (no limit); a mismatch aborts.
fun build_rate_limit(period_ms: Option<u64>, amount: Option<u256>): Option<RateLimit> {
    assert!(period_ms.is_some() == amount.is_some(), EBadRateLimit);
    if (period_ms.is_none()) return option::none();

    let period_ms = *period_ms.borrow();
    let limit = *amount.borrow();
    // A zero period resets the window on every spend; a zero amount spends nothing.
    assert!(period_ms > 0 && limit > 0, EBadRateLimit);
    option::some(RateLimit::FixedWindow {
        period_ms,
        limit,
        spent: 0,
        window_start_ms: 0,
    })
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
    // we do not allow unlimited allowances (TODO: Do we?)
    assert!(lifetime_cap.is_some() || rate_limit.is_some(), ENoLimit);
    // Either a hard end date or bounded drain velocity.
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
