// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A provider-neutral price adapter over Pyth price feeds.
///
/// The adapter normalizes a provider price into a single `Price` type and
/// centralizes the safety checks a high-stakes consumer needs: a staleness
/// bound, a confidence-interval bound, a deviation bound against a reference,
/// and a non-aborting fallback between a primary and a secondary feed. Keeping
/// the `Price` type and the guards provider-neutral means a Switchboard reader
/// can be added as a sibling `price_from_switchboard` without changing consumers.
module oracle_adapter::price_adapter;

use pyth::i64;
use pyth::price::{Self as pyth_price, Price as PythPrice};
use pyth::price_info::PriceInfoObject;
use pyth::pyth;
use sui::clock::{Self, Clock};

const EPriceNegative: u64 = 1;
const EZeroPrice: u64 = 2;
const EConfidenceTooWide: u64 = 3;
const EPriceDeviates: u64 = 4;
const EPriceStale: u64 = 5;

/// A normalized, provider-neutral price. The magnitude is non-negative; the
/// exponent is base-10 and carried separately because provider feeds report
/// prices as `magnitude * 10^expo` with a per-feed exponent. `conf` is the
/// confidence interval in the same units as `mag`. `timestamp` is the publish
/// time in seconds.
public struct Price has copy, drop, store {
    mag: u64,
    expo_magnitude: u64,
    expo_negative: bool,
    conf: u64,
    timestamp: u64,
}

/// Read a Pyth price, rejecting it if it is older than `max_age_secs`, and
/// normalize it. This delegates the staleness gate to Pyth's own
/// `get_price_no_older_than`, which aborts on a stale feed.
public fun price_from_pyth(price_info: &PriceInfoObject, clock: &Clock, max_age_secs: u64): Price {
    let p = pyth::get_price_no_older_than(price_info, clock, max_age_secs);
    from_pyth_price(&p)
}

/// Read the primary feed if it is fresh, otherwise fall back to the secondary.
///
/// Move cannot catch an abort, so this reads the primary with the unchecked
/// `get_price_unsafe` and tests freshness without aborting. Only if the primary
/// is stale does it call the checked reader on the fallback, which aborts if the
/// fallback is also stale. A consumer gets a fresh price or a clear abort, never
/// a silently stale one.
public fun price_or_fallback(
    primary: &PriceInfoObject,
    fallback: &PriceInfoObject,
    clock: &Clock,
    max_age_secs: u64,
): Price {
    let raw = pyth::get_price_unsafe(primary);
    let p = from_pyth_price(&raw);
    if (is_fresh(&p, clock, max_age_secs)) {
        p
    } else {
        price_from_pyth(fallback, clock, max_age_secs)
    }
}

fun from_pyth_price(p: &PythPrice): Price {
    let raw = pyth_price::get_price(p);
    // Consumers value collateral and settle in positive prices; a negative or
    // zero price is never valid input, so reject it at the boundary.
    assert!(!i64::get_is_negative(&raw), EPriceNegative);
    let mag = i64::get_magnitude_if_positive(&raw);
    assert!(mag > 0, EZeroPrice);
    let expo = pyth_price::get_expo(p);
    let expo_negative = i64::get_is_negative(&expo);
    let expo_magnitude = if (expo_negative) {
        i64::get_magnitude_if_negative(&expo)
    } else {
        i64::get_magnitude_if_positive(&expo)
    };
    Price {
        mag,
        expo_magnitude,
        expo_negative,
        conf: pyth_price::get_conf(p),
        timestamp: pyth_price::get_timestamp(p),
    }
}

/// True if the price's publish time is within `max_age_secs` of the clock. Uses
/// the same strict bound and absolute difference as Pyth's own staleness check,
/// so a price that passes here would also pass `get_price_no_older_than`.
public fun is_fresh(p: &Price, clock: &Clock, max_age_secs: u64): bool {
    let now = clock::timestamp_ms(clock) / 1000;
    let age = if (now > p.timestamp) now - p.timestamp else p.timestamp - now;
    age < max_age_secs
}

/// Abort unless the price is fresh. Use when you hold a `Price` you did not read
/// through `price_from_pyth` (for example one read with `get_price_unsafe`).
public fun assert_fresh(p: &Price, clock: &Clock, max_age_secs: u64) {
    assert!(is_fresh(p, clock, max_age_secs), EPriceStale);
}

/// Abort if the confidence interval is wider than `max_conf_bps` of the price. A
/// blown-out confidence interval means the providers disagree, which is a signal
/// to pause rather than act on the midpoint.
public fun assert_confidence(p: &Price, max_conf_bps: u64) {
    assert!(
        (p.conf as u128) * 10000 <= (p.mag as u128) * (max_conf_bps as u128),
        EConfidenceTooWide,
    );
}

/// Abort if the price deviates from `ref_mag` by more than `max_dev_bps`. Use
/// against a second source or a time-averaged reference to catch a single-feed
/// spike before it drives a liquidation. Both values must share the exponent.
public fun assert_within_deviation(p: &Price, ref_mag: u64, max_dev_bps: u64) {
    let diff = if (p.mag > ref_mag) p.mag - ref_mag else ref_mag - p.mag;
    assert!((diff as u128) * 10000 <= (ref_mag as u128) * (max_dev_bps as u128), EPriceDeviates);
}

public fun magnitude(p: &Price): u64 { p.mag }

public fun confidence(p: &Price): u64 { p.conf }

public fun timestamp(p: &Price): u64 { p.timestamp }

public fun expo_magnitude(p: &Price): u64 { p.expo_magnitude }

public fun expo_is_negative(p: &Price): bool { p.expo_negative }

#[test_only]
public fun new_for_testing(
    mag: u64,
    expo_magnitude: u64,
    expo_negative: bool,
    conf: u64,
    timestamp: u64,
): Price {
    Price { mag, expo_magnitude, expo_negative, conf, timestamp }
}

#[test_only]
public fun pyth_price_for_testing(
    mag: u64,
    negative: bool,
    expo: u64,
    conf: u64,
    ts: u64,
): PythPrice {
    pyth_price::new(i64::new(mag, negative), conf, i64::new(expo, true), ts)
}

#[test_only]
public fun normalize_for_testing(p: &PythPrice): Price { from_pyth_price(p) }
