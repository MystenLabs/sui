// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::kiosk_test_utils {
    use std::vector;
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, ID, UID};
    use sui::package::{Self, Publisher};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer_policy::{Self as policy, TransferPolicy, TransferPolicyCap};
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};

    struct OTW has drop {}
    struct Asset has key, store { id: UID }

    /// Prepare: dummy context
    public fun ctx(): TxContext { tx_context::dummy() }

    /// Prepare: accounts
    /// Alice, Bob and my favorite guy - Carl
    public fun folks(): (address, address, address) { (@0xA11CE, @0xB0B, @0xCA51) }

    /// Get the Publisher object.
    public fun get_publisher(ctx: &mut TxContext): Publisher {
        package::test_claim(OTW {}, ctx)
    }

    /// Prepare: TransferPolicy<Asset>
    public fun get_policy(ctx: &mut TxContext): (TransferPolicy<Asset>, TransferPolicyCap<Asset>) {
        let publisher = get_publisher(ctx);
        let (policy, cap) = policy::new(&publisher, ctx);
        return_publisher(publisher);
        (policy, cap)
    }

    /// Prepare: Get Sui
    public fun get_sui(amount: u64, ctx: &mut TxContext): Coin<SUI> {
        coin::mint_for_testing(amount, ctx)
    }

    /// Prepare: Asset
    public fun get_asset(ctx: &mut TxContext): (Asset, ID) {
        let uid = object::new(ctx);
        let id = object::uid_to_inner(&uid);
        (Asset { id: uid }, id)
    }

    /// Prepare: Kiosk
    public fun get_kiosk(ctx: &mut TxContext): (Kiosk, KioskOwnerCap) {
        kiosk::new(ctx)
    }

    public fun return_publisher(publisher: Publisher) {
        package::burn_publisher(publisher)
    }

    /// Cleanup: TransferPolicy
    public fun return_policy(policy: TransferPolicy<Asset>, cap: TransferPolicyCap<Asset>, ctx: &mut TxContext): u64 {
        let profits = policy::destroy_and_withdraw(policy, cap, ctx);
        coin::burn_for_testing(profits)
    }

    /// Cleanup: Kiosk
    public fun return_kiosk(kiosk: Kiosk, cap: KioskOwnerCap, ctx: &mut TxContext): u64 {
        let profits = kiosk::close_and_withdraw(kiosk, cap, ctx);
        coin::burn_for_testing(profits)
    }

    /// Cleanup: vector<Asset>
    public fun return_assets(assets: vector<Asset>) {
        while (vector::length(&assets) > 0) {
            let Asset { id } = vector::pop_back(&mut assets);
            object::delete(id)
        };

        vector::destroy_empty(assets)
    }
}
