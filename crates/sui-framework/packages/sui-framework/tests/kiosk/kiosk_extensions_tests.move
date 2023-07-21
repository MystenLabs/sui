// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Example of an extension which is authorized to place assets into the Kiosk.
module sui::kiosk_place_ext {
    use sui::kiosk::{Self, Kiosk};

    /// The type of the Extension.
    struct Extension has drop {}

    /// Drop an asset into the `Kiosk` using the Extension API.
    public fun place<T: key + store>(self: &mut Kiosk, item: T) {
        kiosk::ext_place(Extension {}, self, item)
    }
}

#[test_only]
/// Example of an extension which does not require any permissions but uses the
/// extension storage to store its state in a protected and centralized place.
module sui::kiosk_marketplace_ext {
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use sui::transfer_policy::TransferRequest;
    use sui::tx_context::TxContext;
    use sui::coin::Coin;
    use sui::object::ID;
    use sui::sui::SUI;
    use sui::bag;

    struct Extension has drop {}

    /// List an item on a marketplace.
    /// This method requires the `KioskOwnerCap` as only owner is authorized to
    /// list and make offers in the Kiosk.
    public fun list<T: key + store>(
        self: &mut Kiosk,
        cap: &KioskOwnerCap,
        item: ID,
        price: u64,
        ctx: &mut TxContext
    ) {
        let purchase_cap = kiosk::list_with_purchase_cap<T>(self, cap, item, price, ctx);
        let storage = kiosk::ext_storage_mut(Extension {}, self);
        bag::add(storage, item, purchase_cap);

        // emit an event? (not implemented)
    }

    /// Purchase an item and pay the marketplace fee.
    public fun purchase<T: key + store>(
        self: &mut Kiosk,
        item: ID,
        payment: Coin<SUI>,
    ): (T, TransferRequest<T>) {
        let storage = kiosk::ext_storage_mut(Extension {}, self);
        let purchase_cap = bag::remove(storage, item);

        // collect a fee? (not implemented)
        // store it in the Bag? (not implemented)

        kiosk::purchase_with_cap(self, purchase_cap, payment)
    }
}

#[test_only]
module sui::kiosk_extensions_tests {
    use sui::kiosk;
    use sui::kiosk_test_utils::{Self as test, Asset};
    use sui::kiosk_place_ext::{Self as place_ext, Extension as PlaceExt};

    #[test, expected_failure(abort_code = kiosk::EExtensionDisabled)]
    fun test_ext_place_fail_extension_not_installed() {
        let ctx = &mut test::ctx();
        let (kiosk, _cap) = test::get_kiosk(ctx);
        let (asset, _asset_id) = test::get_asset(ctx);

        place_ext::place(&mut kiosk, asset);

        abort 1337
    }

    #[test]
    fun test_add_extension() {
        let ctx = &mut test::ctx();
        let (kiosk, cap) = test::get_kiosk(ctx);
        let (asset, asset_id) = test::get_asset(ctx);

        kiosk::add_extension_for_testing<PlaceExt>(&mut kiosk, &cap, 0, ctx);
        place_ext::place(&mut kiosk, asset);

        assert!(kiosk::has_item(&kiosk, asset_id), 0);
        assert!(kiosk::has_extension<PlaceExt>(&kiosk), 1);

        let asset = kiosk::take<Asset>(&mut kiosk, &cap, asset_id);

        test::return_assets(vector[ asset ]);
        test::return_kiosk(kiosk, cap, ctx);
    }
}
