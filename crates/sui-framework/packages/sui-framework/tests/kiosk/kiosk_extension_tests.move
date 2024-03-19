// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::kiosk_marketplace_ext {
    use sui::bag;
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, ID};
    use sui::tx_context::TxContext;
    use sui::kiosk_extension as ext;
    use sui::kiosk::{Self, KioskOwnerCap, Kiosk, PurchaseCap};
    use sui::transfer_policy::{Self as policy, TransferPolicy, TransferRequest};

    /// Trying to access an owner-only action.
    const ENotOwner: u64 = 0;
    /// Trying to purchase an item with an incorrect amount of SUI.
    const EIncorrectAmount: u64 = 1;
    /// Trying to accept a bid from an incorrect Kiosk.
    const EIncorrectKiosk: u64 = 2;
    /// Trying to use an extension that is not installed.
    const ENotInstalled: u64 = 3;

    /// The Extension Witness.
    struct Ext<phantom Market> has drop {}

    /// A Bid on an item of type `T`.
    struct Bid<phantom T> has copy, store, drop {}

    /// Add the `Marketplace` extension to the given `Kiosk`.
    ///
    /// Requests all permissions: `b011` - `place` and `lock` to perform collection bidding.
    public fun add<Market>(kiosk: &mut Kiosk, cap: &KioskOwnerCap, ctx: &mut TxContext) {
        ext::add(Ext<Market> {}, kiosk, cap, 3, ctx)
    }

    // === Collection Bidding ===

    /// Collection bidding: the Kiosk Owner offers a bid (in SUI) for an item of type `T`.
    ///
    /// There can be only one bid per type.
    public fun bid<Market, T: key + store>(
        kiosk: &mut Kiosk, cap: &KioskOwnerCap, bid: Coin<SUI>
    ) {
        assert!(kiosk::has_access(kiosk, cap), ENotOwner);
        assert!(ext::is_installed<Ext<Market>>(kiosk), ENotInstalled);

        bag::add(
            ext::storage_mut(Ext<Market> {}, kiosk),
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
            ext::storage_mut(Ext<Market> {}, destination),
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
        if (lock) ext::lock(Ext<Market> {}, destination, item, policy)
        else ext::place(Ext<Market> {}, destination, item, policy);

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
            ext::storage_mut(Ext<Market> {}, kiosk),
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
            ext::storage_mut(Ext<Market> {}, kiosk),
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
            ext::storage_mut(Ext<Market> {}, kiosk),
            item_id
        );

        kiosk::return_purchase_cap(kiosk, purchase_cap);
    }
}


#[test_only]
module sui::kiosk_extensions_tests {
    use sui::kiosk_test_utils::{Self as test};
    use sui::kiosk_extension as ext;
    use sui::kiosk;

    /// The `Ext` witness to use for testing.
    struct Extension has drop {}

    // === Default Behavior ===

    #[test]
    fun test_default_behavior() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        ext::add(Extension {}, &mut kiosk, &owner_cap, 3, ctx);

        assert!(ext::is_installed<Extension>(&kiosk), 0);
        assert!(ext::is_enabled<Extension>(&kiosk), 0);
        assert!(ext::can_place<Extension>(&kiosk), 0);
        assert!(ext::can_lock<Extension>(&kiosk), 0);

        ext::disable<Extension>(&mut kiosk, &owner_cap);

        assert!(ext::is_installed<Extension>(&kiosk), 0);
        assert!(!ext::is_enabled<Extension>(&kiosk), 0);
        assert!(!ext::can_place<Extension>(&kiosk), 0);
        assert!(!ext::can_lock<Extension>(&kiosk), 0);

        ext::enable<Extension>(&mut kiosk, &owner_cap);

        assert!(ext::is_installed<Extension>(&kiosk), 0);
        assert!(ext::is_enabled<Extension>(&kiosk), 0);
        assert!(ext::can_place<Extension>(&kiosk), 0);
        assert!(ext::can_lock<Extension>(&kiosk), 0);

        test::return_kiosk(kiosk, owner_cap, ctx);
    }

    // === EExtensionNotAllowed ===

    // methods tested:
    // - `ext::place` (not allowed | only lock)
    // - `ext::lock` (not allowed | only place)

    #[test, expected_failure(abort_code = sui::kiosk_extension::EExtensionNotAllowed)]
    fun test_lock_not_allowed() {
        let ctx = &mut test::ctx();
        let (policy, _policy_cap) = test::get_policy(ctx);
        let (asset, _asset_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        ext::add(Extension {}, &mut kiosk, &owner_cap, 0, ctx);
        ext::lock(Extension {}, &mut kiosk, asset, &policy);

        abort 1337
    }

    #[test, expected_failure(abort_code = sui::kiosk_extension::EExtensionNotAllowed)]
    fun test_lock_not_allowed_but_place() {
        let ctx = &mut test::ctx();
        let (policy, _policy_cap) = test::get_policy(ctx);
        let (asset, _asset_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        ext::add(Extension {}, &mut kiosk, &owner_cap, 1, ctx);
        ext::lock(Extension {}, &mut kiosk, asset, &policy);

        abort 1337
    }

    #[test, expected_failure(abort_code = sui::kiosk_extension::EExtensionNotAllowed)]
    fun test_place_not_allowed() {
        let ctx = &mut test::ctx();
        let (policy, _policy_cap) = test::get_policy(ctx);
        let (asset, _asset_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        ext::add(Extension {}, &mut kiosk, &owner_cap, 0, ctx);
        ext::place(Extension {}, &mut kiosk, asset, &policy);

        abort 1337
    }

    #[test]
    fun test_place_allowed_with_lock() {
        let ctx = &mut test::ctx();
        let (policy, policy_cap) = test::get_policy(ctx);
        let (asset, asset_id) = test::get_asset(ctx);
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        ext::add(Extension {}, &mut kiosk, &owner_cap, 2, ctx);
        ext::place(Extension {}, &mut kiosk, asset, &policy);

        let asset = kiosk::take(&mut kiosk, &owner_cap, asset_id);

        test::return_kiosk(kiosk, owner_cap, ctx);
        test::return_policy(policy, policy_cap, ctx);
        test::return_assets(vector[ asset ]);
    }

    // === EExtensionNotInstalled ===

    // methods tested:
    // - `ext::remove`
    // - `ext::storage`
    // - `ext::storage_mut`
    // - `ext::enable`
    // - `ext::disable`
    // - `ext::lock`
    // - `ext::place`

    #[test, expected_failure(abort_code = sui::kiosk_extension::EExtensionNotInstalled)]
    fun test_enable_not_installed() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        ext::enable<Extension>(&mut kiosk, &owner_cap);

        abort 1337
    }

    #[test, expected_failure(abort_code = sui::kiosk_extension::EExtensionNotInstalled)]
    fun test_disable_not_installed() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        ext::disable<Extension>(&mut kiosk, &owner_cap);

        abort 1337
    }

    #[test, expected_failure(abort_code = sui::kiosk_extension::EExtensionNotInstalled)]
    fun test_remove_not_installed() {
        let ctx = &mut test::ctx();
        let (kiosk, owner_cap) = test::get_kiosk(ctx);

        ext::remove<Extension>(&mut kiosk, &owner_cap);

        abort 1337
    }

    #[test, expected_failure(abort_code = sui::kiosk_extension::EExtensionNotInstalled)]
    fun test_storage_not_installed() {
        let ctx = &mut test::ctx();
        let (kiosk, _owner_cap) = test::get_kiosk(ctx);

        let _ = ext::storage(Extension {}, &kiosk);

        abort 1337
    }

    #[test, expected_failure(abort_code = sui::kiosk_extension::EExtensionNotInstalled)]
    fun test_storage_mut_not_installed() {
        let ctx = &mut test::ctx();
        let (kiosk, _owner_cap) = test::get_kiosk(ctx);

        let _ = ext::storage_mut(Extension {}, &mut kiosk);

        abort 1337
    }

    #[test, expected_failure(abort_code = sui::kiosk_extension::EExtensionNotInstalled)]
    fun test_lock_not_installed() {
        let ctx = &mut test::ctx();
        let (policy, _policy_cap) = test::get_policy(ctx);
        let (asset, _asset_id) = test::get_asset(ctx);
        let (kiosk, _owner_cap) = test::get_kiosk(ctx);

        ext::lock(Extension {}, &mut kiosk, asset, &policy);

        abort 1337
    }

    #[test, expected_failure(abort_code = sui::kiosk_extension::EExtensionNotInstalled)]
    fun test_place_not_installed() {
        let ctx = &mut test::ctx();
        let (policy, _policy_cap) = test::get_policy(ctx);
        let (asset, _asset_id) = test::get_asset(ctx);
        let (kiosk, _owner_cap) = test::get_kiosk(ctx);

        ext::place(Extension {}, &mut kiosk, asset, &policy);

        abort 1337
    }
}
