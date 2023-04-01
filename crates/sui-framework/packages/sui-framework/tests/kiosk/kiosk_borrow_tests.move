// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Tests for borrowing mechanics.
module sui::kiosk_borrow_tests {
    use sui::kiosk_test_utils::{Self as utils, Asset};
    use sui::kiosk;

    const AMT: u64 = 1000;

    // === borrow ===

    #[test]
    fun test_borrow() {
        let ctx = &mut utils::ctx();
        let (item, id) = utils::get_asset(ctx);
        let (kiosk, cap) = utils::get_kiosk(ctx);

        kiosk::place(&mut kiosk, &cap, item);
        let _item_ref = kiosk::borrow<Asset>(&kiosk, &cap, id);

        kiosk::list<Asset>(&mut kiosk, &cap, id, AMT);
        let _item_ref = kiosk::borrow<Asset>(&kiosk, &cap, id);

        let item = kiosk::take<Asset>(&mut kiosk, &cap, id);
        utils::return_assets(vector[ item ]);
        utils::return_kiosk(kiosk, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotOwner)]
    fun test_borrow_fail_not_owner() {
        let ctx = &mut utils::ctx();
        let (_item, id) = utils::get_asset(ctx);
        let (kiosk, _cap) = utils::get_kiosk(ctx);
        let (_kiosk, cap) = utils::get_kiosk(ctx);

        let _item_ref = kiosk::borrow<Asset>(&kiosk, &cap, id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_borrow_fail_item_not_found() {
        let ctx = &mut utils::ctx();
        let (_item, id) = utils::get_asset(ctx);
        let (kiosk, cap) = utils::get_kiosk(ctx);

        let _item_ref = kiosk::borrow<Asset>(&kiosk, &cap, id);

        abort 1337
    }

    // === borrow mut ===

    #[test]
    fun test_borrow_mut() {
        let ctx = &mut utils::ctx();
        let (item, id) = utils::get_asset(ctx);
        let (kiosk, cap) = utils::get_kiosk(ctx);

        kiosk::place(&mut kiosk, &cap, item);
        let _item_mut = kiosk::borrow_mut<Asset>(&mut kiosk, &cap, id);

        let item = kiosk::take<Asset>(&mut kiosk, &cap, id);
        utils::return_assets(vector[ item ]);
        utils::return_kiosk(kiosk, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotOwner)]
    fun test_borrow_mut_fail_not_owner() {
        let ctx = &mut utils::ctx();
        let (_item, id) = utils::get_asset(ctx);
        let (kiosk, _cap) = utils::get_kiosk(ctx);
        let (_kiosk, cap) = utils::get_kiosk(ctx);
        let _item_mut = kiosk::borrow_mut<Asset>(&mut kiosk, &cap, id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_borrow_mut_fail_item_not_found() {
        let ctx = &mut utils::ctx();
        let (_item, id) = utils::get_asset(ctx);
        let (kiosk, cap) = utils::get_kiosk(ctx);
        let _item_mut = kiosk::borrow_mut<Asset>(&mut kiosk, &cap, id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemIsListed)]
    fun test_borrow_mut_fail_item_is_listed() {
        let ctx = &mut utils::ctx();
        let (item, id) = utils::get_asset(ctx);
        let (kiosk, cap) = utils::get_kiosk(ctx);

        kiosk::place_and_list(&mut kiosk, &cap, item, AMT);
        let _item_mut = kiosk::borrow_mut<Asset>(&mut kiosk, &cap, id);

        abort 1337
    }

    // === borrow val ===

    #[test]
    fun test_borrow_val() {
        let ctx = &mut utils::ctx();
        let (item, id) = utils::get_asset(ctx);
        let (kiosk, cap) = utils::get_kiosk(ctx);

        kiosk::place(&mut kiosk, &cap, item);
        let (item, potato) = kiosk::borrow_val<Asset>(&mut kiosk, &cap, id);
        assert!(sui::object::id(&item) == id, 0);
        kiosk::return_val(&mut kiosk, item, potato);
        assert!(kiosk::has_item(&kiosk, id), 0);

        let item = kiosk::take<Asset>(&mut kiosk, &cap, id);
        utils::return_assets(vector[ item ]);
        utils::return_kiosk(kiosk, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotOwner)]
    fun test_borrow_val_fail_not_owner() {
        let ctx = &mut utils::ctx();
        let (_item, id) = utils::get_asset(ctx);
        let (kiosk, _cap) = utils::get_kiosk(ctx);
        let (_kiosk, cap) = utils::get_kiosk(ctx);
        let (_item, _borrow) = kiosk::borrow_val<Asset>(&mut kiosk, &cap, id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_borrow_val_fail_item_not_found() {
        let ctx = &mut utils::ctx();
        let (_item, id) = utils::get_asset(ctx);
        let (kiosk, cap) = utils::get_kiosk(ctx);
        let (_item, _borrow) = kiosk::borrow_val<Asset>(&mut kiosk, &cap, id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemIsListed)]
    fun test_borrow_val_fail_item_is_listed() {
        let ctx = &mut utils::ctx();
        let (item, id) = utils::get_asset(ctx);
        let (kiosk, cap) = utils::get_kiosk(ctx);

        kiosk::place_and_list(&mut kiosk, &cap, item, AMT);
        let (_item, _borrow) = kiosk::borrow_val<Asset>(&mut kiosk, &cap, id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EWrongKiosk)]
    fun test_borrow_val_fail_wrong_kiosk() {
        let ctx = &mut utils::ctx();
        let (item_1, id_1) = utils::get_asset(ctx);
        let (kiosk_1, cap_1) = utils::get_kiosk(ctx);
        kiosk::place(&mut kiosk_1, &cap_1, item_1);

        let (item_2, id_2) = utils::get_asset(ctx);
        let (kiosk_2, cap_2) = utils::get_kiosk(ctx);
        kiosk::place(&mut kiosk_2, &cap_2, item_2);

        let (item, _borrow) = kiosk::borrow_val<Asset>(&mut kiosk_1, &cap_1, id_1);
        let (_item, borrow) = kiosk::borrow_val<Asset>(&mut kiosk_2, &cap_2, id_2);

        kiosk::return_val(&mut kiosk_1, item, borrow);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemMismatch)]
    fun test_borrow_val_fail_item_mismatch() {
        let ctx = &mut utils::ctx();
        let (item_1, id_1) = utils::get_asset(ctx);
        let (kiosk_1, cap_1) = utils::get_kiosk(ctx);
        kiosk::place(&mut kiosk_1, &cap_1, item_1);

        let (item_2, id_2) = utils::get_asset(ctx);
        let (kiosk_2, cap_2) = utils::get_kiosk(ctx);
        kiosk::place(&mut kiosk_2, &cap_2, item_2);

        let (item, _borrow) = kiosk::borrow_val<Asset>(&mut kiosk_1, &cap_1, id_1);
        let (_item, borrow) = kiosk::borrow_val<Asset>(&mut kiosk_2, &cap_2, id_2);

        kiosk::return_val(&mut kiosk_2, item, borrow);

        abort 1337
    }
}
