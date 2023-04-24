// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Test extensions functionality. Make sure it's not breaking the kiosk as well
/// as the new extensions functions are working as expected.
module sui::kiosk_extension_tests {
    use sui::kiosk;
    // use sui::kiosk_permissions as auth;
    use sui::kiosk_test_utils::{Self as test, Asset};

    struct Ext has drop {}

    #[test]
    fun ext_test_add_get_remove_extension() {
        let ctx = test::ctx();
        let (kiosk, kiosk_cap) = test::get_kiosk(&mut ctx);
        let auth = 0;

        kiosk::add_extension_for_testing<Ext>(&mut kiosk, &kiosk_cap, auth);
        assert!(kiosk::get_extension_permissions<Ext>(&kiosk) == 0, 0);
        kiosk::remove_extension<Ext>(&mut kiosk, &kiosk_cap);

        test::return_kiosk(kiosk, kiosk_cap, &mut ctx);
    }

    #[test]
    fun ext_test_all_permissions() {
        let ctx = test::ctx();
        let (kiosk, kiosk_cap) = test::get_kiosk(&mut ctx);
        let (policy, policy_cap) = test::get_policy(&mut ctx);

        // 0111 - mask for all permissions (Place, Borrow, Lock)
        kiosk::add_extension_for_testing<Ext>(&mut kiosk, &kiosk_cap, 0xF);
        assert!(kiosk::get_extension_permissions<Ext>(&kiosk) == 0xF, 0);

        let (asset, item_id) = test::get_asset(&mut ctx);

        // check whether the extension can place the asset
        kiosk::place_as_extension(Ext {}, &mut kiosk, asset);
        kiosk::has_item(&kiosk, item_id);

        // check whether the extension can borrow the asset
        let _asset: &Asset = kiosk::borrow_as_extension(Ext {}, &mut kiosk, item_id);

        // take the asset as owner
        let asset = kiosk::take(&mut kiosk, &kiosk_cap, item_id);

        // check whether the extension can lock the asset
        kiosk::lock_as_extension(Ext {}, &mut kiosk, &policy, asset);
        kiosk::list<Asset>(&mut kiosk, &kiosk_cap, item_id, 0);

        let (asset, req) = kiosk::purchase(&mut kiosk, item_id, test::get_sui(0, &mut ctx));
        sui::transfer_policy::confirm_request(&policy, req);

        test::return_assets(vector[ asset ]);
        test::return_policy(policy, policy_cap, &mut ctx);
        test::return_kiosk(kiosk, kiosk_cap, &mut ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EExtNotPermitted)]
    fun ext_test_place_fail() {
        let ctx = test::ctx();
        let (asset, _) = test::get_asset(&mut ctx);
        let (kiosk, kiosk_cap) = test::get_kiosk(&mut ctx);

        kiosk::add_extension_for_testing<Ext>(&mut kiosk, &kiosk_cap, 0x0);
        kiosk::place_as_extension(Ext {}, &mut kiosk, asset);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EExtNotPermitted)]
    fun ext_test_borrow_fail() {
        let ctx = test::ctx();
        let (asset, item_id) = test::get_asset(&mut ctx);
        let (kiosk, kiosk_cap) = test::get_kiosk(&mut ctx);

        kiosk::place(&mut kiosk, &kiosk_cap, asset);

        kiosk::add_extension_for_testing<Ext>(&mut kiosk, &kiosk_cap, 0x4);
        let _: &Asset = kiosk::borrow_as_extension(Ext {}, &kiosk, item_id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EExtNotPermitted)]
    fun ext_test_lock_fail() {
        let ctx = test::ctx();
        let (asset, _item_id) = test::get_asset(&mut ctx);
        let (kiosk, kiosk_cap) = test::get_kiosk(&mut ctx);
        let (policy, _policy_cap) = test::get_policy(&mut ctx);

        kiosk::add_extension_for_testing<Ext>(&mut kiosk, &kiosk_cap, 0x0);
        kiosk::lock_as_extension(Ext {}, &mut kiosk, &policy, asset);

        abort 1337
    }
}
