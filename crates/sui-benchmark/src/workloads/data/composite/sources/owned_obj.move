// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An address-owned object that can be frozen or deleted, and a helper that mints owned
/// TEST_COIN coins for merging.
module basics::owned_obj {
    use basics::test_coin::{Self, TestCoinCap};

    public struct OwnedObj has key, store {
        id: UID,
        value: u64,
    }

    public fun create(recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(OwnedObj { id: object::new(ctx), value: 0 }, recipient);
    }

    public fun set_value(o: &mut OwnedObj, value: u64) {
        o.value = value;
    }

    public fun value(o: &OwnedObj): u64 {
        o.value
    }

    public fun freeze_obj(o: OwnedObj) {
        transfer::public_freeze_object(o);
    }

    public fun delete(o: OwnedObj) {
        let OwnedObj { id, value: _ } = o;
        id.delete();
    }

    /// Mint two TEST_COIN coins to `recipient`.
    public fun mint_coins(cap: &mut TestCoinCap, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(test_coin::mint(cap, 1, ctx), recipient);
        transfer::public_transfer(test_coin::mint(cap, 1, ctx), recipient);
    }
}
