// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Kiosk testing strategy:
/// - [ ] test purchase flow
/// - [ ] test purchase cap flow
/// - [ ] test withdraw methods
module sui::kiosk_tests {
    use sui::kiosk_test_utils::{Self as test, Asset};
    use sui::sui::SUI;
    use sui::coin;

    const AMT: u64 = 10_000;

    #[test]
    fun test_set_owner_custom() {
        let ctx = &mut test::ctx();
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        let old_owner = kiosk.owner();
        kiosk.set_owner(&owner_cap, ctx);
        assert!(kiosk.owner() == old_owner);

        kiosk.set_owner_custom(&owner_cap, @0xA11CE);
        assert!(kiosk.owner() != old_owner);
        assert!(kiosk.owner() == @0xA11CE);

        test::return_kiosk(kiosk, owner_cap, ctx);
    }

    #[test]
    fun test_place_and_take() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, policy_cap) = test::get_policy(ctx);

        kiosk.place(&owner_cap, asset);

        assert!(kiosk.has_item(item_id));
        let asset = kiosk.take(&owner_cap, item_id);
        assert!(!kiosk.has_item(item_id));

        test::return_policy(policy, policy_cap, ctx);
        test::return_kiosk(kiosk, owner_cap, ctx);
        test::return_assets(vector[ asset ]);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemLocked)]
    fun test_taking_not_allowed() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, _policy_cap) = test::get_policy(ctx);

        kiosk.lock(&owner_cap, &policy, asset);
        let _asset = kiosk.take<Asset>(&owner_cap, item_id);
        abort 1337
    }

    #[test]
    fun test_purchase() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, policy_cap) = test::get_policy(ctx);

        kiosk.place_and_list(&owner_cap, asset, AMT);
        assert!(kiosk.is_listed(item_id));
        let payment = coin::mint_for_testing<SUI>(AMT, ctx);
        let (asset, request) = kiosk.purchase(item_id, payment);
        assert!(!kiosk.is_listed(item_id));
        policy.confirm_request(request);

        test::return_kiosk(kiosk, owner_cap, ctx);
        test::return_assets(vector[ asset ]);
        test::return_policy(policy, policy_cap, ctx);
    }

    #[test]
    fun test_delist() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, policy_cap) = test::get_policy(ctx);

        kiosk.place_and_list(&owner_cap, asset, AMT);
        assert!(kiosk.is_listed(item_id));
        kiosk.delist<Asset>(&owner_cap, item_id);
        assert!(!kiosk.is_listed(item_id));
        let asset = kiosk.take(&owner_cap, item_id);

        test::return_kiosk(kiosk, owner_cap, ctx);
        test::return_assets(vector[ asset ]);
        test::return_policy(policy, policy_cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotListed)]
    fun test_delist_not_listed() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk.place(&owner_cap, asset);
        kiosk.delist<Asset>(&owner_cap, item_id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EListedExclusively)]
    fun test_delist_listed_exclusively() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk.place(&owner_cap, asset);
        let _cap = kiosk.list_with_purchase_cap<Asset>(&owner_cap, item_id, 100, ctx);

        kiosk.delist<Asset>(&owner_cap, item_id);
        abort 1337
    }

    #[allow(unused_field)]
    public struct WrongAsset has key, store { id: sui::object::UID }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_delist_wrong_type() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk.place(&owner_cap, asset);
        kiosk.delist<WrongAsset>(&owner_cap, item_id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_delist_no_item() {
        let ctx = &mut test::ctx();
        let (_asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk.delist<Asset>(&owner_cap, item_id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EIncorrectAmount)]
    fun test_purchase_wrong_amount() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, _policy_cap) = test::get_policy(ctx);

        kiosk.place_and_list(&owner_cap, asset, AMT);
        let payment = coin::mint_for_testing<SUI>(AMT + 1, ctx);
        let (_asset, request) = kiosk.purchase(item_id, payment);
        policy.confirm_request(request);

        abort 1337
    }

    #[test]
    fun test_purchase_cap() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, policy_cap) = test::get_policy(ctx);

        kiosk.place(&owner_cap, asset);
        let purchase_cap = kiosk.list_with_purchase_cap(&owner_cap, item_id, AMT, ctx);
        let payment = coin::mint_for_testing<SUI>(AMT, ctx);
        assert!(kiosk.is_listed_exclusively(item_id));
        let (asset, request) = kiosk.purchase_with_cap(purchase_cap, payment);
        assert!(!kiosk.is_listed_exclusively(item_id));
        policy.confirm_request(request);

        test::return_kiosk(kiosk, owner_cap, ctx);
        test::return_assets(vector[ asset ]);
        test::return_policy(policy, policy_cap, ctx);
    }

    #[test]
    fun test_purchase_cap_return() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, policy_cap) = test::get_policy(ctx);

        kiosk.place(&owner_cap, asset);
        let purchase_cap = kiosk.list_with_purchase_cap<test::Asset>(&owner_cap, item_id, AMT, ctx);
        kiosk.return_purchase_cap(purchase_cap);
        let asset = kiosk.take(&owner_cap, item_id);

        test::return_kiosk(kiosk, owner_cap, ctx);
        test::return_assets(vector[ asset ]);
        test::return_policy(policy, policy_cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_list_no_item_fail() {
        let ctx = &mut test::ctx();
        let (_asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk.list<Asset>(&owner_cap, item_id, AMT);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_list_with_purchase_cap_no_item_fail() {
        let ctx = &mut test::ctx();
        let (_asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        let _purchase_cap = kiosk.list_with_purchase_cap<Asset>(&owner_cap, item_id, AMT, ctx);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EAlreadyListed)]
    fun test_purchase_cap_already_listed_fail() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk.place_and_list(&owner_cap, asset, AMT);
        let _purchase_cap = kiosk.list_with_purchase_cap<test::Asset>(&owner_cap, item_id, AMT, ctx);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EListedExclusively)]
    fun test_purchase_cap_issued_list_fail() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk.place(&owner_cap, asset);
        let purchase_cap = kiosk.list_with_purchase_cap<test::Asset>(&owner_cap, item_id, AMT, ctx);
        kiosk.list<test::Asset>(&owner_cap, item_id, AMT);
        kiosk.return_purchase_cap(purchase_cap);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotEmpty)]
    fun test_kiosk_has_items() {
        let ctx = &mut test::ctx();
        let (_policy, _cap) = test::get_policy(ctx);
        let (asset, _item_id) = test::get_asset(ctx);
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk.place(&owner_cap, asset);
        test::return_kiosk(kiosk, owner_cap, ctx);

        abort 1337
    }

    #[test]
    fun test_withdraw_default() {
        let ctx = &mut test::ctx();
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);
        let profits = kiosk.withdraw(&owner_cap, option::none(), ctx);

        profits.burn_for_testing();
        test::return_kiosk(kiosk, owner_cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotEnough)]
    fun test_withdraw_more_than_there_is() {
        let ctx = &mut test::ctx();
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);
        let _profits = kiosk.withdraw(&owner_cap, option::some(100), ctx);

        abort 1337
    }

    #[test]
    fun test_disallow_extensions_access_as_owner() {
        let ctx = &mut test::ctx();
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk.set_allow_extensions(&owner_cap, false);
        let _uid_mut = kiosk.uid_mut_as_owner(&owner_cap);
        test::return_kiosk(kiosk, owner_cap, ctx);
    }

    #[test]
    fun test_uid_access() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        let uid = kiosk.uid();
        assert!(sui::object::uid_to_inner(uid) == sui::object::id(&kiosk));

        test::return_kiosk(kiosk, owner_cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EUidAccessNotAllowed)]
    fun test_disallow_extensions_uid_mut() {
        let ctx = &mut test::ctx();
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk.set_allow_extensions(&owner_cap, false);
        let _ = kiosk.uid_mut();

        abort 1337
    }

    #[test]
    fun test_disallow_extensions_uid_available() {
        let ctx = &mut test::ctx();
        let (mut kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk.set_allow_extensions(&owner_cap, false);
        let _ = kiosk.uid();

        test::return_kiosk(kiosk, owner_cap, ctx);
    }
}
