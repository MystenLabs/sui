// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// These tests mainly check authorization logic, as the game module
/// delegates to other modules for actual logic. Tests for that logic
/// are found in the corresponding test modules.
module turnip_town::game_tests {
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::test_scenario::{Self as ts, Scenario};
    use sui::transfer_policy::TransferPolicy;
    use turnip_town::game;
    use turnip_town::turnip::{Self, Turnip};
    use turnip_town::water;

    const ALICE: address = @0xA;

    /// Pretend one-time witness for test purposes.
    public struct OTW() has drop;

    #[test]
    fun add_remove() {
        let mut ts = ts::begin(ALICE);
        let (mut kiosk, cap) = kiosk::new(ts.ctx());

        game::add(&mut kiosk, &cap, ts.ctx());
        game::remove(&mut kiosk, &cap, ts.ctx());

        kiosk.close_and_withdraw(cap, ts.ctx()).burn_for_testing();
        ts.end();
    }

    #[test]
    #[expected_failure(abort_code = game::EAlreadyInstalled)]
    fun double_add() {
        let mut ts = ts::begin(ALICE);
        let (mut kiosk, cap) = kiosk::new(ts.ctx());

        game::add(&mut kiosk, &cap, ts.ctx());
        game::add(&mut kiosk, &cap, ts.ctx());
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = game::ENotAuthorized)]
    fun add_unauthorized() {
        let mut ts = ts::begin(ALICE);
        let (mut k0, _c) = kiosk::new(ts.ctx());
        let (mut _k, c1) = kiosk::new(ts.ctx());

        game::add(&mut k0, &c1, ts.ctx());
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = game::ENotAuthorized)]
    fun remove_unauthorized() {
        let mut ts = ts::begin(ALICE);
        let (mut k0, c0) = kiosk::new(ts.ctx());
        let (mut _k, c1) = kiosk::new(ts.ctx());

        game::add(&mut k0, &c0, ts.ctx());
        game::remove(&mut k0, &c1, ts.ctx());
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = game::ENotInstalled)]
    fun remove_nonexistent() {
        let mut ts = ts::begin(ALICE);
        let (mut kiosk, cap) = kiosk::new(ts.ctx());

        game::remove(&mut kiosk, &cap, ts.ctx());
        abort 0
    }

    #[test]
    #[expected_failure(abort_code = game::ENotInstalled)]
    fun uninstalled_sow() {
        let mut ts = ts::begin(ALICE);
        let (mut kiosk, cap) = kiosk::new(ts.ctx());
        game::sow(&mut kiosk, &cap, 0, 0, ts.ctx());
        abort 0
    }

    #[test]
    fun authorized_sow() {
        let (mut ts, mut kiosk, cap) = setup();
        game::sow(&mut kiosk, &cap, 0, 0, ts.ctx());
        tear_down(ts, kiosk, cap);
    }

    #[test]
    #[expected_failure(abort_code = game::ENotAuthorized)]
    fun unauthorized_sow() {
        let (mut ts, mut k0, _c) = setup();
        let (_k, c1) = kiosk::new(ts.ctx());
        game::sow(&mut k0, &c1, 0, 0, ts.ctx());
        abort 0
    }

    #[test]
    fun authorized_fetch_water() {
        let (mut ts, mut kiosk, cap) = setup();
        let _ = game::fetch_water(&mut kiosk, &cap, 1, ts.ctx());
        tear_down(ts, kiosk, cap);
    }

    #[test]
    #[expected_failure(abort_code = game::ENotAuthorized)]
    fun unauthorized_fetch_water() {
        let (mut ts, mut k0, _c) = setup();
        let (_k, c1) = kiosk::new(ts.ctx());
        let _ = game::fetch_water(&mut k0, &c1, 1, ts.ctx());
        abort 0
    }

    #[test]
    fun authorized_harvest() {
        let (mut ts, mut kiosk, cap) = setup();
        game::sow(&mut kiosk, &cap, 0, 0, ts.ctx());
        game::water(&mut kiosk, 0, 0, water::for_test(150), ts.ctx());

        // Wait some time for the turnip to grow -- otherwise it won't
        // be big enough.
        ts.next_epoch(ALICE);

        // Calling simulate is not required every epoch, but it
        // shouldn't do any harm.
        game::simulate(&mut kiosk, ts.ctx());

        ts.next_epoch(ALICE);
        ts.next_epoch(ALICE);

        let policy: TransferPolicy<Turnip> = ts.take_shared();
        let id = game::harvest(&mut kiosk, &policy, 0, 0, ts.ctx());
        ts::return_shared(policy);

        let turnip: Turnip = kiosk.take(&cap, id);
        assert!(turnip.size() == 60);
        assert!(turnip.freshness() == 90_00);

        turnip.consume();
        tear_down(ts, kiosk, cap);
    }

    fun setup(): (Scenario, Kiosk, KioskOwnerCap) {
        let mut ts = ts::begin(ALICE);
        let (mut kiosk, cap) = kiosk::new(ts.ctx());

        turnip::test_init(ts.ctx());
        game::add(&mut kiosk, &cap, ts.ctx());

        (ts, kiosk, cap)
    }

    fun tear_down(mut ts: Scenario, mut kiosk: Kiosk, cap: KioskOwnerCap) {
        game::remove(&mut kiosk, &cap, ts.ctx());
        kiosk.close_and_withdraw(cap, ts.ctx()).burn_for_testing();
        ts.end();
    }
}
