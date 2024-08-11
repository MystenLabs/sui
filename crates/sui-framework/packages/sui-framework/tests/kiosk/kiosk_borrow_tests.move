// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Tests for borrowing mechanics.
module sui::kiosk_borrow_tests {
    use sui::kiosk_test_utils::{Self as utils, Asset};

    const AMT: u64 = 1000;

    // === borrow ===

    #[test]
    fun test_borrow() {
        let ctx = &mut utils::ctx();
        let (item, id) = utils::get_asset(ctx);
        let (mut kiosk, cap) = utils::get_kiosk(ctx);

        kiosk.place(&cap, item);
        let _item_ref = kiosk.borrow<Asset>(&cap, id);

        kiosk.list<Asset>(&cap, id, AMT);
        let _item_ref = kiosk.borrow<Asset>(&cap, id);

        let item = kiosk.take<Asset>(&cap, id);
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

        let _item_ref = kiosk.borrow<Asset>(&cap, id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_borrow_fail_item_not_found() {
        let ctx = &mut utils::ctx();
        let (_item, id) = utils::get_asset(ctx);
        let (kiosk, cap) = utils::get_kiosk(ctx);

        let _item_ref: &Asset = &kiosk[&cap, id];

        abort 1337
    }

    // === borrow mut ===

    #[test]
    fun test_borrow_mut() {
        let ctx = &mut utils::ctx();
        let (item, id) = utils::get_asset(ctx);
        let (mut kiosk, cap) = utils::get_kiosk(ctx);

        kiosk.place(&cap, item);
        let _item_mut: &mut Asset = &mut kiosk[&cap, id];

        let item = kiosk.take<Asset>(&cap, id);
        utils::return_assets(vector[ item ]);
        utils::return_kiosk(kiosk, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotOwner)]
    fun test_borrow_mut_fail_not_owner() {
        let ctx = &mut utils::ctx();
        let (_item, id) = utils::get_asset(ctx);
        let (mut kiosk, _cap) = utils::get_kiosk(ctx);
        let (_kiosk, cap) = utils::get_kiosk(ctx);
        let _item_mut: &mut Asset = &mut kiosk[&cap, id];

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_borrow_mut_fail_item_not_found() {
        let ctx = &mut utils::ctx();
        let (_item, id) = utils::get_asset(ctx);
        let (mut kiosk, cap) = utils::get_kiosk(ctx);
        let _item_mut: &mut Asset = &mut kiosk[&cap, id];

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemIsListed)]
    fun test_borrow_mut_fail_item_is_listed() {
        let ctx = &mut utils::ctx();
        let (item, id) = utils::get_asset(ctx);
        let (mut kiosk, cap) = utils::get_kiosk(ctx);

        kiosk.place_and_list(&cap, item, AMT);
        let _item_mut: &mut Asset = &mut kiosk[&cap, id];

        abort 1337
    }

    // === borrow val ===

    #[test]
    fun test_borrow_val() {
        let ctx = &mut utils::ctx();
        let (item, id) = utils::get_asset(ctx);
        let (mut kiosk, cap) = utils::get_kiosk(ctx);

        kiosk.place(&cap, item);
        let (item, potato) = kiosk.borrow_val<Asset>(&cap, id);
        assert!(sui::object::id(&item) == id);
        kiosk.return_val(item, potato);
        assert!(kiosk.has_item(id));

        let item = kiosk.take<Asset>(&cap, id);
        utils::return_assets(vector[ item ]);
        utils::return_kiosk(kiosk, cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotOwner)]
    fun test_borrow_val_fail_not_owner() {
        let ctx = &mut utils::ctx();
        let (_item, id) = utils::get_asset(ctx);
        let (mut kiosk, _cap) = utils::get_kiosk(ctx);
        let (_kiosk, cap) = utils::get_kiosk(ctx);
        let (_item, _borrow) = kiosk.borrow_val<Asset>(&cap, id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_borrow_val_fail_item_not_found() {
        let ctx = &mut utils::ctx();
        let (_item, id) = utils::get_asset(ctx);
        let (mut kiosk, cap) = utils::get_kiosk(ctx);
        let (_item, _borrow) = kiosk.borrow_val<Asset>(&cap, id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemIsListed)]
    fun test_borrow_val_fail_item_is_listed() {
        let ctx = &mut utils::ctx();
        let (item, id) = utils::get_asset(ctx);
        let (mut kiosk, cap) = utils::get_kiosk(ctx);

        kiosk.place_and_list(&cap, item, AMT);
        let (_item, _borrow) = kiosk.borrow_val<Asset>(&cap, id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EWrongKiosk)]
    fun test_borrow_val_fail_wrong_kiosk() {
        let ctx = &mut utils::ctx();
        let (item_1, id_1) = utils::get_asset(ctx);
        let (mut kiosk_1, cap_1) = utils::get_kiosk(ctx);
        kiosk_1.place(&cap_1, item_1);

        let (item_2, id_2) = utils::get_asset(ctx);
        let (mut kiosk_2, cap_2) = utils::get_kiosk(ctx);
        kiosk_2.place(&cap_2, item_2);

        let (item, _borrow) = kiosk_1.borrow_val<Asset>(&cap_1, id_1);
        let (_item, borrow) = kiosk_2.borrow_val<Asset>(&cap_2, id_2);

        kiosk_1.return_val(item, borrow);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemMismatch)]
    fun test_borrow_val_fail_item_mismatch() {
        let ctx = &mut utils::ctx();
        let (item_1, id_1) = utils::get_asset(ctx);
        let (mut kiosk_1, cap_1) = utils::get_kiosk(ctx);
        kiosk_1.place(&cap_1, item_1);

        let (item_2, id_2) = utils::get_asset(ctx);
        let (mut kiosk_2, cap_2) = utils::get_kiosk(ctx);
        kiosk_2.place(&cap_2, item_2);

        let (item, _borrow) = kiosk_1.borrow_val<Asset>(&cap_1, id_1);
        let (_item, borrow) = kiosk_2.borrow_val<Asset>(&cap_2, id_2);

        kiosk_2.return_val(item, borrow);

        abort 1337
    }
}
