// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This module contains a simple `unlocker` module which enables creators to
/// `unlock` (as opposed to `lock`) assets without fulfilling the default set
/// of requirements (rules / policies).
module kiosk::unlock_example {
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::package::{Self, Publisher};
    use sui::object::{Self, ID, UID};
    use sui::tx_context::TxContext;
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap
    };
    use sui::coin;

    /// Trying to destroy the Unlocker object while not being its publisher.
    const ENotPublisher: u64 = 0;

    /// An unlocker is a special type of object which can be used to unlock
    /// assets with another `TransferPolicy`.
    struct Unlocker<phantom T> has key, store {
        id: UID,
        policy: TransferPolicy<T>,
        cap: TransferPolicyCap<T>,
    }

    /// Create a new `Unlocker` object; can either be used directly or wrapped
    /// into a custom object (with matching logic).
    public fun new<T: key + store>(
        publisher: &Publisher,
        ctx: &mut TxContext
    ): Unlocker<T> {
        let (policy, cap) = policy::new(publisher, ctx);
        Unlocker { cap, policy, id: object::new(ctx) }
    }

    /// Unlocks the item. Can only be performed by the owner of the Kiosk.
    public fun unlock<T: key + store>(
        self: &Unlocker<T>,
        kiosk: &mut Kiosk,
        cap: &KioskOwnerCap,
        item_id: ID,
        ctx: &mut TxContext
    ): T {
        let p_cap = kiosk::list_with_purchase_cap(kiosk, cap, item_id, 0, ctx);
        let (item, request) = kiosk::purchase_with_cap(kiosk, p_cap, coin::zero(ctx));

        policy::confirm_request(&self.policy, request);
        item
    }

    /// Destroy the unlocker object. Can only be performed by the publisher
    /// of the type `T`.
    public fun destroy<T: key + store>(
        self: Unlocker<T>,
        publisher: &Publisher,
        ctx: &mut TxContext
    ) {
        assert!(package::from_package<T>(publisher), ENotPublisher);

        let Unlocker { id, policy, cap } = self;
        let zero = policy::destroy_and_withdraw(policy, cap, ctx);
        coin::destroy_zero(zero);
        object::delete(id);
    }

    #[test_only]
    use sui::kiosk_test_utils::{
        Self as test,
        Asset
    };

    #[test]
    fun default_scenario() {
        let ctx = &mut test::ctx();
        let publisher = test::get_publisher(ctx);
        let (kiosk, kiosk_cap) = test::get_kiosk(ctx);

        // default policy (needed for locking)
        let (policy, policy_cap) = test::get_policy(ctx);
        let unlocker = new<Asset>(&publisher, ctx);
        let (asset, asset_id) = test::get_asset(ctx);

        // an asset is locked, unless transferred it cannot be unlocked
        kiosk::lock(&mut kiosk, &kiosk_cap, &policy, asset);

        // one magic line that does it all
        let asset = unlock(&unlocker, &mut kiosk, &kiosk_cap, asset_id, ctx);

        // the end, unlocker is destroyed
        destroy(unlocker, &publisher, ctx);

        test::return_policy(policy, policy_cap, ctx);
        test::return_kiosk(kiosk, kiosk_cap, ctx);
        test::return_assets(vector[ asset ]);
        test::return_publisher(publisher);
    }
}
