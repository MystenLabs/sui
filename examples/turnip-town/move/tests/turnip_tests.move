// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module turnip_town::turnip_tests {
    use sui::test_scenario as ts;
    use turnip_town::turnip;

    #[test]
    fun fresh() {
        let mut ts = ts::begin(@0x0);
        let turnip = turnip::fresh(ts.ctx());

        assert!(turnip.size() == 0);
        assert!(turnip.freshness() == 100_00);
        assert!(!turnip.can_harvest());
        assert!(turnip.is_fresh());

        turnip.consume();
        ts.end();
    }

    #[test]
    fun simulate() {
        let mut ts = ts::begin(@0x0);
        let mut turnip = turnip::fresh(ts.ctx());

        let mut w = 10;
        turnip.simulate(&mut w, 1);
        assert!(turnip.size() == 10);
        assert!(turnip.freshness() == 100_00);
        assert!(w == 0);

        // Leave water behind after simulation.
        w = 100;
        turnip.simulate(&mut w, 1);
        assert!(turnip.size() == 30);
        assert!(turnip.freshness() == 100_00);
        assert!(w == 70);

        // Simulate multiple days.
        // Day 1: 150 - 30 - 20 = 100
        // Day 2: 100 - 50 - 20 = 30
        // Day 3:  30 - 30      = 0 (freshness halves).

        w = 150;
        turnip.simulate(&mut w, 3);
        assert!(turnip.size() == 70);
        assert!(turnip.freshness() == 50_00);
        assert!(w == 0);

        // Recovering some freshness
        w = 100;
        turnip.simulate(&mut w, 1);
        assert!(turnip.size() == 90);
        assert!(turnip.freshness() == 70_00);
        assert!(w == 10);

        // Growth while water-logged.
        // Day 1: 1000 -  90 - 20 = 890 fresh: 35_00
        // Day 2:  890 - 110 - 20 = 760 fresh: 17_50
        // Day 3:  760 - 130 - 20 = 610 fresh:  8_75

        w = 1000;
        turnip.simulate(&mut w, 3);
        assert!(turnip.size() == 150);
        assert!(turnip.freshness() == 8_75);
        assert!(w == 610);

        // Growth while in drought.
        // Day 1: fresh: 4_37
        // Day 2: fresh: 2_18
        // Day 3: fresh: 1_09
        // Day 4: fresh:   54
        // Day 4: fresh:   27

        w = 100;
        turnip.simulate(&mut w, 5);
        assert!(turnip.size() == 150);
        assert!(turnip.freshness() == 27);
        assert!(w == 0);

        turnip.consume();
        ts.end();
    }
}
