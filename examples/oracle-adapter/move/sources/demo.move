// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A thin demo consumer that reads a price through the adapter and emits it as an
/// event, so a walkthrough can observe the value the adapter validated and
/// returned. Real consumers call `price_adapter` functions directly rather than
/// emitting; this module exists only to make an executed read observable.
module oracle_adapter::demo;

use oracle_adapter::price_adapter;
use pyth::price_info::PriceInfoObject;
use sui::clock::Clock;
use sui::event;

public struct PriceRead has copy, drop {
    magnitude: u64,
    expo_magnitude: u64,
    expo_negative: bool,
    confidence: u64,
    timestamp: u64,
}

/// Read the Pyth feed through the adapter with a staleness bound, then emit the
/// normalized price. Aborts if the feed is older than `max_age_secs`.
public fun read_and_emit(price_info: &PriceInfoObject, clock: &Clock, max_age_secs: u64) {
    let p = price_adapter::price_from_pyth(price_info, clock, max_age_secs);
    event::emit(PriceRead {
        magnitude: price_adapter::magnitude(&p),
        expo_magnitude: price_adapter::expo_magnitude(&p),
        expo_negative: price_adapter::expo_is_negative(&p),
        confidence: price_adapter::confidence(&p),
        timestamp: price_adapter::timestamp(&p),
    });
}
