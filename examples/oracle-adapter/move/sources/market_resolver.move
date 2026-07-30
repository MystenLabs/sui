// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Price-based resolution for a parametric market: freeze a settlement price
/// from the oracle exactly once, at or after an expiry time. This is the
/// resolution pattern for outcomes that are a deterministic function of a price
/// (an expiry settlement, a strike, a barrier). Subjective outcomes need an
/// optimistic or dispute process instead, which no oracle resolves for you.
module oracle_adapter::market_resolver;

use oracle_adapter::price_adapter;
use pyth::price_info::PriceInfoObject;
use sui::clock::{Self, Clock};

const ENotExpired: u64 = 1;
const EAlreadySettled: u64 = 2;

public struct Market has key, store {
    id: UID,
    expiry_ms: u64,
    max_age_secs: u64,
    settlement: Option<u64>,
}

public fun new(expiry_ms: u64, max_age_secs: u64, ctx: &mut TxContext): Market {
    Market { id: object::new(ctx), expiry_ms, max_age_secs, settlement: option::none() }
}

/// A market can settle once, at or after expiry. Checking this before reading
/// the oracle keeps the two failure modes, settling early and settling twice,
/// separate from any oracle failure.
public fun assert_resolvable(market: &Market, clock: &Clock) {
    assert!(clock::timestamp_ms(clock) >= market.expiry_ms, ENotExpired);
    assert!(market.settlement.is_none(), EAlreadySettled);
}

/// Freeze the settlement price from the oracle. The adapter's staleness bound
/// applies, so a market cannot settle against a price the transaction did not
/// refresh. After this, the settlement price is fixed and readable forever.
public fun resolve(market: &mut Market, price_info: &PriceInfoObject, clock: &Clock) {
    assert_resolvable(market, clock);
    let p = price_adapter::price_from_pyth(price_info, clock, market.max_age_secs);
    market.settlement.fill(price_adapter::magnitude(&p));
}

public fun settlement(market: &Market): Option<u64> { market.settlement }

public fun is_settled(market: &Market): bool { market.settlement.is_some() }

#[test_only]
public fun settle_for_testing(market: &mut Market, price: u64) {
    market.settlement.fill(price);
}

#[test_only]
public fun destroy_for_testing(market: Market) {
    let Market { id, expiry_ms: _, max_age_secs: _, settlement: _ } = market;
    object::delete(id);
}
