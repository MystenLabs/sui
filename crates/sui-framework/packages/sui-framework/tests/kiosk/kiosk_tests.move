// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::kiosk_marketplace_ext {
    use sui::bag;
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, ID};
    use sui::tx_context::TxContext;
    use sui::kiosk::{Self, KioskOwnerCap, Kiosk, PurchaseCap};
    use sui::transfer_policy::{Self as policy, TransferPolicy, TransferRequest};

    /// Trying to access an owner-only action.
    const ENotOwner: u64 = 0;
    /// Trying to purchase an item with an incorrect amount of SUI.
    const EIncorrectAmount: u64 = 1;
    /// Trying to accept a bid from an incorrect Kiosk.
    const EIncorrectKiosk: u64 = 2;

    /// The Extension Witness.
    struct Ext<phantom Market> has drop {}

    /// A Bid on an item of type `T`.
    struct Bid<phantom T> has copy, store, drop {}

    /// A Hot-Potato ensuring the item is placed or locked in the destination.
    struct PlaceOrLock<phantom T> { id: ID }

    /// Add the `Marketplace` extension to the given `Kiosk`.
    ///
    /// Requests all permissions: `b011` - `place` and `lock` to perform collection bidding.
    public fun add<Market>(kiosk: &mut Kiosk, cap: &KioskOwnerCap, ctx: &mut TxContext) {
        kiosk::add_extension(Ext<Market> {}, kiosk, cap, 3, vector[], ctx)
    }

    // === Collection Bidding ===

    /// Collection bidding: the Kiosk Owner offers a bid (in SUI) for an item of type `T`.
    ///
    /// There can be only one bid per type.
    public fun bid<Market, T: key + store>(
        kiosk: &mut Kiosk, cap: &KioskOwnerCap, bid: Coin<SUI>
    ) {
        assert!(kiosk::has_access(kiosk, cap), ENotOwner);

        bag::add(
            kiosk::ext_storage_mut(Ext<Market> {}, kiosk),
            Bid<T> {},
            bid
        );
    }

    /// Collection bidding: offer the `T` and receive the bid.
    public fun accept_bid<Market, T: key + store>(
        destination: &mut Kiosk,
        source: &mut Kiosk,
        purchase_cap: PurchaseCap<T>,
        policy: &TransferPolicy<T>,
        lock: bool
    ): (TransferRequest<T>, TransferRequest<Market>) {
        let bid: Coin<SUI> = bag::remove(
            kiosk::ext_storage_mut(Ext<Market> {}, destination),
            Bid<T> {}
        );

        // form the request while we have all the data (not yet consumed)
        let market_request = policy::new_request(
            kiosk::purchase_cap_item(&purchase_cap), coin::value(&bid), object::id(source)
        );

        assert!(kiosk::purchase_cap_kiosk(&purchase_cap) == object::id(source), EIncorrectKiosk);
        assert!(kiosk::purchase_cap_min_price(&purchase_cap) <= coin::value(&bid), EIncorrectAmount);

        let (item, request) = kiosk::purchase_with_cap(source, purchase_cap, bid);

        // lock or place the item into the Kiosk (chosen by the caller, however
        // TransferPolicy<T> will ensure that the right action is taken).
        if (lock) kiosk::ext_lock(Ext<Market> {}, destination, item, policy)
        else kiosk::ext_place(Ext<Market> {}, destination, item);

        (
            request,
            market_request
        )
    }

    // === List / Delist / Purchase ===

    /// List an item for sale.
    public fun list<Market, T: key + store>(
        kiosk: &mut Kiosk, cap: &KioskOwnerCap, item_id: ID, price: u64, ctx: &mut TxContext
    ) {
        let purchase_cap = kiosk::list_with_purchase_cap<T>(
            kiosk, cap, item_id, price, ctx
        );

        bag::add(
            kiosk::ext_storage_mut(Ext<Market> {}, kiosk),
            item_id,
            purchase_cap
        );
    }

    /// Purchase an item from the Kiosk while following the Marketplace policy.
    public fun purchase<Market, T: key + store>(
        kiosk: &mut Kiosk,
        item_id: ID,
        payment: Coin<SUI>,
    ): (T, TransferRequest<T>, TransferRequest<Market>) {
        let purchase_cap: PurchaseCap<T> = bag::remove(
            kiosk::ext_storage_mut(Ext<Market> {}, kiosk),
            item_id
        );

        assert!(coin::value(&payment) == kiosk::purchase_cap_min_price(&purchase_cap), EIncorrectAmount);
        let market_request = policy::new_request(item_id, coin::value(&payment), object::id(kiosk));
        let (item, request) = kiosk::purchase_with_cap(kiosk, purchase_cap, payment);

        (
            item,
            request,
            market_request
        )
    }

    /// Delist an item.
    /// Note: the extension needs to be "trusted" - i.e. having PurchaseCap stored
    /// in the extension storage is not absolutely secure.
    public fun delist<Market, T: key + store>(
        kiosk: &mut Kiosk,
        cap: &KioskOwnerCap,
        item_id: ID,
    ) {
        assert!(kiosk::has_access(kiosk, cap), ENotOwner);
        let purchase_cap: PurchaseCap<T> = bag::remove(
            kiosk::ext_storage_mut(Ext<Market> {}, kiosk),
            item_id
        );

        kiosk::return_purchase_cap(kiosk, purchase_cap);
    }
}


#[test_only]
module sui::kiosk_extensions_tests {
    use sui::kiosk_test_utils::{Self as test};
    use sui::kiosk;
    use sui::bag;

    /// The `Ext` witness to use for testing.
    struct Extension has drop {}

    #[test]
    fun test_add_extension() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::add_extension(
            Extension {},
            &mut kiosk,
            &owner_cap,
            0,
            vector[],
            ctx
        );

        let bag_mut = kiosk::ext_storage_mut(Extension {}, &mut kiosk);

        bag::add(bag_mut, b"haha", b"yall");
        test::return_kiosk(kiosk, owner_cap, ctx);
    }
}


#[test_only]
/// Kiosk testing strategy:
/// - [ ] test purchase flow
/// - [ ] test purchase cap flow
/// - [ ] test withdraw methods
module sui::kiosk_tests {
    use sui::kiosk_test_utils::{Self as test, Asset};
    use sui::transfer_policy as policy;
    use sui::sui::SUI;
    use sui::kiosk;
    use sui::coin;

    const AMT: u64 = 10_000;

    #[test]
    fun test_set_owner_custom() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        let old_owner = kiosk::owner(&kiosk);
        kiosk::set_owner(&mut kiosk, &owner_cap, ctx);
        assert!(kiosk::owner(&kiosk) == old_owner, 0);

        kiosk::set_owner_custom(&mut kiosk, &owner_cap, @0xA11CE);
        assert!(kiosk::owner(&kiosk) != old_owner, 0);
        assert!(kiosk::owner(&kiosk) == @0xA11CE, 0);

        test::return_kiosk(kiosk, owner_cap, ctx);
    }

    #[test]
    fun test_place_and_take() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, policy_cap) = test::get_policy(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);

        assert!(kiosk::has_item(&kiosk, item_id), 0);
        let asset = kiosk::take(&mut kiosk, &owner_cap, item_id);
        assert!(!kiosk::has_item(&kiosk, item_id), 0);

        test::return_policy(policy, policy_cap, ctx);
        test::return_kiosk(kiosk, owner_cap, ctx);
        test::return_assets(vector[ asset ]);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemLocked)]
    fun test_taking_not_allowed() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, _policy_cap) = test::get_policy(ctx);

        kiosk::lock(&mut kiosk, &owner_cap, &policy, asset);
        let _asset = kiosk::take<Asset>(&mut kiosk, &owner_cap, item_id);
        abort 1337
    }

    #[test]
    fun test_purchase() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, policy_cap) = test::get_policy(ctx);

        kiosk::place_and_list(&mut kiosk, &owner_cap, asset, AMT);
        assert!(kiosk::is_listed(&kiosk, item_id), 0);
        let payment = coin::mint_for_testing<SUI>(AMT, ctx);
        let (asset, request) = kiosk::purchase(&mut kiosk, item_id, payment);
        assert!(!kiosk::is_listed(&kiosk, item_id), 0);
        policy::confirm_request(&mut policy, request);

        test::return_kiosk(kiosk, owner_cap, ctx);
        test::return_assets(vector[ asset ]);
        test::return_policy(policy, policy_cap, ctx);
    }

    #[test]
    fun test_delist() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, policy_cap) = test::get_policy(ctx);

        kiosk::place_and_list(&mut kiosk, &owner_cap, asset, AMT);
        assert!(kiosk::is_listed(&kiosk, item_id), 0);
        kiosk::delist<Asset>(&mut kiosk, &owner_cap, item_id);
        assert!(!kiosk::is_listed(&kiosk, item_id), 0);
        let asset = kiosk::take(&mut kiosk, &owner_cap, item_id);

        test::return_kiosk(kiosk, owner_cap, ctx);
        test::return_assets(vector[ asset ]);
        test::return_policy(policy, policy_cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotListed)]
    fun test_delist_not_listed() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        kiosk::delist<Asset>(&mut kiosk, &owner_cap, item_id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EListedExclusively)]
    fun test_delist_listed_exclusively() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        let _cap = kiosk::list_with_purchase_cap<Asset>(
            &mut kiosk, &owner_cap, item_id, 100, ctx
        );

        kiosk::delist<Asset>(&mut kiosk, &owner_cap, item_id);
        abort 1337
    }

    struct WrongAsset has key, store { id: sui::object::UID }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_delist_wrong_type() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        kiosk::delist<WrongAsset>(&mut kiosk, &owner_cap, item_id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_delist_no_item() {
        let ctx = &mut test::ctx();
        let (_asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::delist<Asset>(&mut kiosk, &owner_cap, item_id);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EIncorrectAmount)]
    fun test_purchase_wrong_amount() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, _policy_cap) = test::get_policy(ctx);

        kiosk::place_and_list(&mut kiosk, &owner_cap, asset, AMT);
        let payment = coin::mint_for_testing<SUI>(AMT + 1, ctx);
        let (_asset, request) = kiosk::purchase(&mut kiosk, item_id, payment);
        policy::confirm_request(&mut policy, request);

        abort 1337
    }

    #[test]
    fun test_purchase_cap() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, policy_cap) = test::get_policy(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        let purchase_cap = kiosk::list_with_purchase_cap(&mut kiosk, &owner_cap, item_id, AMT, ctx);
        let payment = coin::mint_for_testing<SUI>(AMT, ctx);
        assert!(kiosk::is_listed_exclusively(&kiosk, item_id), 0);
        let (asset, request) = kiosk::purchase_with_cap(&mut kiosk, purchase_cap, payment);
        assert!(!kiosk::is_listed_exclusively(&kiosk, item_id), 0);
        policy::confirm_request(&mut policy, request);

        test::return_kiosk(kiosk, owner_cap, ctx);
        test::return_assets(vector[ asset ]);
        test::return_policy(policy, policy_cap, ctx);
    }

    #[test]
    fun test_purchase_cap_return() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);
        let (policy, policy_cap) = test::get_policy(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        let purchase_cap = kiosk::list_with_purchase_cap<test::Asset>(&mut kiosk, &owner_cap, item_id, AMT, ctx);
        kiosk::return_purchase_cap(&mut kiosk, purchase_cap);
        let asset = kiosk::take(&mut kiosk, &owner_cap, item_id);

        test::return_kiosk(kiosk, owner_cap, ctx);
        test::return_assets(vector[ asset ]);
        test::return_policy(policy, policy_cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_list_no_item_fail() {
        let ctx = &mut test::ctx();
        let (_asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::list<Asset>(&mut kiosk, &owner_cap, item_id, AMT);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EItemNotFound)]
    fun test_list_with_purchase_cap_no_item_fail() {
        let ctx = &mut test::ctx();
        let (_asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        let _purchase_cap = kiosk::list_with_purchase_cap<Asset>(&mut kiosk, &owner_cap, item_id, AMT, ctx);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EAlreadyListed)]
    fun test_purchase_cap_already_listed_fail() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::place_and_list(&mut kiosk, &owner_cap, asset, AMT);
        let _purchase_cap = kiosk::list_with_purchase_cap<test::Asset>(&mut kiosk, &owner_cap, item_id, AMT, ctx);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EListedExclusively)]
    fun test_purchase_cap_issued_list_fail() {
        let ctx = &mut test::ctx();
        let (asset, item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        let purchase_cap = kiosk::list_with_purchase_cap<test::Asset>(&mut kiosk, &owner_cap, item_id, AMT, ctx);
        kiosk::list<test::Asset>(&mut kiosk, &owner_cap, item_id, AMT);
        kiosk::return_purchase_cap(&mut kiosk, purchase_cap);

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotEmpty)]
    fun test_kiosk_has_items() {
        let ctx = &mut test::ctx();
        let (_policy, _cap) = test::get_policy(ctx);
        let (asset, _item_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::place(&mut kiosk, &owner_cap, asset);
        test::return_kiosk(kiosk, owner_cap, ctx);

        abort 1337
    }

    #[test]
    fun test_withdraw_default() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);
        let profits = kiosk::withdraw(&mut kiosk, &owner_cap, std::option::none(), ctx);

        coin::burn_for_testing(profits);
        test::return_kiosk(kiosk, owner_cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::ENotEnough)]
    fun test_withdraw_more_than_there_is() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);
        let _profits = kiosk::withdraw(&mut kiosk, &owner_cap, std::option::some(100), ctx);

        abort 1337
    }

    #[test]
    fun test_disallow_extensions_access_as_owner() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::set_allow_extensions(&mut kiosk, &owner_cap, false);
        let _uid_mut = kiosk::uid_mut_as_owner(&mut kiosk, &owner_cap);
        test::return_kiosk(kiosk, owner_cap, ctx);
    }

    #[test]
    fun test_uid_access() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        let uid = kiosk::uid(&kiosk);
        assert!(sui::object::uid_to_inner(uid) == sui::object::id(&kiosk), 0);

        test::return_kiosk(kiosk, owner_cap, ctx);
    }

    #[test]
    #[expected_failure(abort_code = sui::kiosk::EUidAccessNotAllowed)]
    fun test_disallow_extensions_uid_mut() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::set_allow_extensions(&mut kiosk, &owner_cap, false);
        let _ = kiosk::uid_mut(&mut kiosk);

        abort 1337
    }

    #[test]
    fun test_disallow_extensions_uid_available() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        kiosk::set_allow_extensions(&mut kiosk, &owner_cap, false);
        let _ = kiosk::uid(&kiosk);

        test::return_kiosk(kiosk, owner_cap, ctx);
    }
}
