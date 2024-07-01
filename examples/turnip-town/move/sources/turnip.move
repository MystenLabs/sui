// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This module defines the `Turnip` NFT, and a transfer policy configured with
/// a royalty.
///
/// Any owner of a turnip can query its properties, but modifications are
/// protected (can only be done by other modules in this package, in particular
/// `field`).
module turnip_town::turnip {
    use sui::math;
    use sui::package::{Self, Publisher};
    use sui::transfer_policy;
    use turnip_town::royalty_policy;

    // === Types ===

    public struct TURNIP has drop {}

    public struct Turnip has key, store {
        id: UID,

        /// Size is measured in its own units.
        size: u64,

        /// Freshness is measured in basis points.
        freshness: u16,
    }

    // === Constants ===

    /// The smallest size that a plant can be harvested at to still get a
    /// turnip.
    const MIN_SIZE: u64 = 50;

    /// Initially, turnips start out maximally fresh.
    const MAX_FRESHNESS_BP: u16 = 100_00;

    /// Freshness recovered in a day when a turnip has just enough water (not
    /// too much, not too little).
    const REFRESH_BP: u16 = 20_00;

    /// Maximum size units that can be grown in a day.
    const MAX_DAILY_GROWTH: u64 = 20;

    /// If there is more than this much water left at the end of the day,
    /// turnips lose their freshness.
    const MAX_STAGNANT_WATER: u64 = 100;

    // === Public Functions ===

    fun init(otw: TURNIP, ctx: &mut TxContext) {
        init_policy(package::claim(otw, ctx), ctx);
    }

    /// Turnips that are below the minimum size cannot be harvested.
    public fun can_harvest(turnip: &Turnip): bool {
        turnip.size >= MIN_SIZE
    }

    public fun size(turnip: &Turnip): u64 {
        turnip.size
    }

    public fun freshness(turnip: &Turnip): u16 {
        turnip.freshness
    }

    public fun is_fresh(turnip: &Turnip): bool {
        turnip.freshness > 0
    }

    public fun consume(turnip: Turnip) {
        let Turnip { id, size: _, freshness: _ } = turnip;
        id.delete();
    }

    // === Protected Functions ===

    /// A brand new turnip (only the `field` module can create these).
    public(package) fun fresh(ctx: &mut TxContext): Turnip {
        Turnip {
            id: object::new(ctx),
            size: 0,
            freshness: 100_00,
        }
    }

    /// Simulate `days` days passing with `turnip` sitting in `water`.
    ///
    /// Turnips need to consume at least their size in water every day and
    /// subsequently can grow up to `MAX_DAILY_GROWTH`, with each unit of growth
    /// requiring a unit of water. At the end of the day they cannot be left in
    /// more than `MAX_STAGNANT_WATER`.
    ///
    /// If the turnip has too little or too much water, its freshness halves at
    /// the end of the day, otherwise, freshness increases by `REFRESH_BP`, up
    /// to `MAX_FRESHNESS_BP`.
    public(package) fun simulate(
        turnip: &mut Turnip,
        water: &mut u64,
        mut days: u64,
    ) {
        while (days > 0) {
            days = days - 1;
            if (*water < turnip.size) {
                turnip.freshness = turnip.freshness / 2;
                *water = 0;
                continue
            };

            *water = *water - turnip.size;
            let growth = math::min(MAX_DAILY_GROWTH, *water);
            turnip.size = turnip.size + growth;
            *water = *water - growth;

            if (*water > MAX_STAGNANT_WATER) {
                turnip.freshness = turnip.freshness / 2;
                continue
            };

            turnip.freshness = turnip.freshness + REFRESH_BP;
            if (turnip.freshness > MAX_FRESHNESS_BP) {
                turnip.freshness = MAX_FRESHNESS_BP;
            }
        }
    }

    // === Private Functions ===

    #[allow(lint(share_owned, self_transfer))]
    fun init_policy(publisher: Publisher, ctx: &mut TxContext) {
        let (mut policy, cap) = transfer_policy::new<Turnip>(&publisher, ctx);

        royalty_policy::set(&mut policy, &cap);
        transfer::public_share_object(policy);
        transfer::public_transfer(cap, ctx.sender());
        publisher.burn();
    }

    // === Test Helpers ===

    #[test_only]
    public struct OTW() has drop;

    #[test_only]
    public fun test_init(ctx: &mut TxContext) {
        init_policy(package::test_claim(OTW(), ctx), ctx);
    }

    #[test_only]
    public fun prepare_for_harvest_for_test(turnip: &mut Turnip) {
        turnip.size = MIN_SIZE + 1;
    }
}
