// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module turnip_town::water_tests {
    use sui::test_scenario as ts;
    use turnip_town::water;

    const ALICE: address = @0xA;

    #[test]
    fun zero() {
        assert!(water::zero().value() == 0);
    }

    #[test]
    fun fetch_water() {
        let mut ts = ts::begin(ALICE);

        let mut well = water::well(ts.ctx());
        let water = well.fetch(water::per_epoch(), ts.ctx());
        assert!(water.value() == water::per_epoch());

        ts.end();
    }

    #[test]
    #[expected_failure(abort_code = water::ENotEnough)]
    fun fetch_too_much_water() {
        let mut ts = ts::begin(ALICE);

        let mut well = water::well(ts.ctx());
        let _ = well.fetch(water::per_epoch(), ts.ctx());
        let _ = well.fetch(1, ts.ctx());

        abort 0
    }

    #[test]
    fun well_replenish() {
        let mut ts = ts::begin(ALICE);

        let mut well = water::well(ts.ctx());
        let water = well.fetch(water::per_epoch(), ts.ctx());
        assert!(water.value() == water::per_epoch());

        ts.next_epoch(ALICE);
        let water = well.fetch(1, ts.ctx());
        assert!(water.value() == 1);

        ts.end();
    }

    #[test]
    fun water_split() {
        let mut water = water::for_test(42);
        let drop = water.split(5);

        assert!(water.value() == 37);
        assert!(drop.value() == 5);
    }

    #[test]
    #[expected_failure(abort_code = water::ENotEnough)]
    fun water_split_too_much() {
        let mut water = water::for_test(42);
        let _ = water.split(43);
    }

    #[test]
    fun water_join() {
        let mut water = water::for_test(42);
        let drop = water.split(5);
        water.join(drop);
        assert!(water.value() == 42);
    }
}
