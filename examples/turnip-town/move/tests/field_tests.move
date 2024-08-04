// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module turnip_town::field_tests {
    use sui::test_scenario as ts;
    use turnip_town::field;
    use turnip_town::water;

    const ALICE: address = @0xA;

    #[test]
    fun burn_empty() {
        let mut ts = ts::begin(ALICE);
        let field = field::new(ts.ctx());
        field.burn(ts.ctx());
        ts.end();
    }

    #[test]
    fun burn_non_harvest() {
        let mut ts = ts::begin(ALICE);
        let mut field = field::new(ts.ctx());
        field.sow(0, 0, ts.ctx());
        field.burn(ts.ctx());
        ts.end();
    }

    #[test]
    #[expected_failure(abort_code = field::ENotEmpty)]
    fun burn_failure() {
        let mut ts = ts::begin(ALICE);
        let mut field = field::new(ts.ctx());

        // Sow a turnip, now the field is not empty.
        field.sow(0, 0, ts.ctx());
        field[0, 0].prepare_for_harvest_for_test();
        field.burn(ts.ctx());
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = field::ENotEmpty)]
    /// The field appears empty, but because of pending simulation activity, it
    /// actually is not.
    fun burn_failure_latent() {
        let mut ts = ts::begin(ALICE);
        let mut field = field::new(ts.ctx());

        // Sow a turnip, now the field is not empty.
        field.sow(0, 0, ts.ctx());
        field.water(0, 0, water::for_test(120), ts.ctx());

        // Advance a number of epochs to allow the turnip to grow
        ts.next_epoch(ALICE);
        ts.next_epoch(ALICE);
        ts.next_epoch(ALICE);

        // This burn will not succeed because the turnip has grown.
        field.burn(ts.ctx());
        abort 0
    }

    #[test]
    fun sow_and_harvest() {
        let mut ts = ts::begin(ALICE);
        let mut field = field::new(ts.ctx());

        field.sow(0, 0, ts.ctx());
        field[0, 0].prepare_for_harvest_for_test();
        let turnip = field.harvest(0, 0, ts.ctx());
        turnip.consume();

        field.burn(ts.ctx());
        ts.end();
    }

    #[test]
    #[expected_failure(abort_code = field::EOutOfBounds)]
    fun sow_out_of_bounds() {
        let mut ts = ts::begin(ALICE);
        let mut field = field::new(ts.ctx());
        field.sow(1000, 1000, ts.ctx());
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = field::EAlreadyFilled)]
    fun sow_overlap() {
        let mut ts = ts::begin(ALICE);
        let mut field = field::new(ts.ctx());
        field.sow(0, 0, ts.ctx());
        field.sow(0, 0, ts.ctx());
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = field::EOutOfBounds)]
    fun harvest_out_of_bounds() {
        let mut ts = ts::begin(ALICE);
        let mut field = field::new(ts.ctx());
        let _turnip = field.harvest(1000, 1000, ts.ctx());
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = field::ETooSmall)]
    fun harvest_too_small() {
        let mut ts = ts::begin(ALICE);
        let mut field = field::new(ts.ctx());

        field.sow(0, 0, ts.ctx());
        let _turnip = field.harvest(0, 0, ts.ctx());
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = field::ENotFilled)]
    fun harvest_non_existent() {
        let mut ts = ts::begin(ALICE);
        let mut field = field::new(ts.ctx());
        let _turnip = field.harvest(0, 0, ts.ctx());
        abort 0
    }

    #[test]
    fun multiple_epochs() {
        let mut ts = ts::begin(ALICE);
        let mut field = field::new(ts.ctx());

        field.sow(0, 0, ts.ctx());
        field.sow(0, 1, ts.ctx());
        field.water(0, 0, water::for_test(60), ts.ctx());
        field.water(0, 1, water::for_test(10), ts.ctx());

        // Advance a couple of epochs.
        ts.next_epoch(ALICE);
        ts.next_epoch(ALICE);

        // The slot doesn't change until it's touched again, even though the
        // epoch has advanced.
        assert!(field[0, 0].size() == 0);
        assert!(field[0, 1].size() == 0);

        // Watering the slot will also update its simulation to account for the
        // epochs that have passed.
        field.water(0, 0, water::zero(), ts.ctx());
        assert!(field[0, 0].size() == 40);
        assert!(field[0, 0].freshness() == 100_00);

        // Other slots are not affected until they are also touched.
        assert!(field[0, 1].size() == 0);

        field.water(0, 1, water::zero(), ts.ctx());
        assert!(field[0, 1].size() == 10);
        assert!(field[0, 1].freshness() == 50_00);

        field.burn(ts.ctx());
        ts.end();
    }

    #[test]
    fun tick_rate() {
        let mut ts = ts::begin(ALICE);

        // Create two fields and plant them identically -- one will be simulated
        // every epoch, and another will be simulated at different epoch
        // boundaries, but the results of their simulations should match.
        let mut f0 = field::new(ts.ctx());
        let mut f1 = field::new(ts.ctx());

        // One turnip gets a good amount of water.
        f0.sow(0, 0, ts.ctx());
        f0.water(0, 0, water::for_test(100), ts.ctx());
        f1.sow(0, 0, ts.ctx());
        f1.water(0, 0, water::for_test(100), ts.ctx());

        // Another turnip gets way too much water.
        f0.sow(0, 1, ts.ctx());
        f0.water(0, 1, water::for_test(1000), ts.ctx());
        f1.sow(0, 1, ts.ctx());
        f1.water(0, 1, water::for_test(1000), ts.ctx());

        // The final turnip gets way too little.
        f0.sow(0, 2, ts.ctx());
        f0.water(0, 2, water::for_test(10), ts.ctx());
        f1.sow(0, 2, ts.ctx());
        f1.water(0, 2, water::for_test(10), ts.ctx());

        ts.next_epoch(ALICE);
        f0.simulate(ts.ctx());

        assert!(f0[0, 0].size() == 20);
        assert!(f0[0, 1].size() == 20);
        assert!(f0[0, 2].size() == 10);
        assert!(f0[0, 0].freshness() == 100_00);
        assert!(f0[0, 1].freshness() == 50_00);
        assert!(f0[0, 2].freshness() == 100_00);

        ts.next_epoch(ALICE);
        f0.simulate(ts.ctx());
        f1.simulate(ts.ctx());

        assert!(f0[0, 0].size() == 40);
        assert!(f0[0, 1].size() == 40);
        assert!(f0[0, 2].size() == 10);
        assert!(f0[0, 0].freshness() == 100_00);
        assert!(f0[0, 1].freshness() == 25_00);
        assert!(f0[0, 2].freshness() == 50_00);

        assert!(f1[0, 0].size() == 40);
        assert!(f1[0, 1].size() == 40);
        assert!(f1[0, 2].size() == 10);
        assert!(f1[0, 0].freshness() == 100_00);
        assert!(f1[0, 1].freshness() == 25_00);
        assert!(f1[0, 2].freshness() == 50_00);

        ts.next_epoch(ALICE);
        f0.simulate(ts.ctx());

        assert!(f0[0, 0].size() == 40);
        assert!(f0[0, 1].size() == 60);
        assert!(f0[0, 2].size() == 10);
        assert!(f0[0, 0].freshness() == 100_00);
        assert!(f0[0, 1].freshness() == 12_50);
        assert!(f0[0, 2].freshness() == 25_00);

        ts.next_epoch(ALICE);
        f0.simulate(ts.ctx());

        assert!(f0[0, 0].size() == 40);
        assert!(f0[0, 1].size() == 80);
        assert!(f0[0, 2].size() == 10);
        assert!(f0[0, 0].freshness() == 50_00);
        assert!(f0[0, 1].freshness() == 6_25);
        assert!(f0[0, 2].freshness() == 12_50);

        // Top-up the turnip we're supposed to be caring for properly.
        f0.water(0, 0, water::for_test(60), ts.ctx());
        f1.water(0, 0, water::for_test(60), ts.ctx());

        ts.next_epoch(ALICE);

        f0.simulate(ts.ctx());
        f1.simulate(ts.ctx());

        assert!(f0[0, 0].size() == 60);
        assert!(f0[0, 1].size() == 100);
        assert!(f0[0, 2].size() == 10);
        assert!(f0[0, 0].freshness() == 70_00);
        assert!(f0[0, 1].freshness() == 3_12);
        assert!(f0[0, 2].freshness() == 6_25);

        assert!(f1[0, 0].size() == 60);
        assert!(f1[0, 1].size() == 100);
        assert!(f1[0, 2].size() == 10);
        assert!(f1[0, 0].freshness() == 70_00);
        assert!(f1[0, 1].freshness() == 3_12);
        assert!(f1[0, 2].freshness() == 6_25);

        f0.destroy_for_test();
        f1.destroy_for_test();
        ts.end();
    }

    #[test]
    fun clean_up() {
        let mut ts = ts::begin(ALICE);
        let mut field = field::new(ts.ctx());

        field.sow(0, 0, ts.ctx());
        field.water(0, 0, water::for_test(10), ts.ctx());

        // Repeatedly supply less than the necessary water -- the turnip will
        // lose freshness over time, until it is cleaned up.
        let mut i = 15;
        while (i > 0) {
            ts.next_epoch(ALICE);
            field.water(0, 0, water::for_test(1), ts.ctx());
            i = i - 1;
        };

        // Eventually the simulation cleans up the turnip.
        assert!(field.is_empty(0, 0));
        field.burn(ts.ctx());
        ts.end();
    }
}
