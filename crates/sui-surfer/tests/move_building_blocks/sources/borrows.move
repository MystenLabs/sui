// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises the `borrow` hot-potato API (Referent / Borrow). The full
/// borrow/put_back cycle is performed inside a single call.
module move_building_blocks::borrows {
    use sui::borrow::{Self, Referent};

    public struct Asset has key, store {
        id: UID,
        value: u64,
    }

    public struct Vault has key, store {
        id: UID,
        referent: Referent<Asset>,
    }

    public fun create_vault(value: u64, ctx: &mut TxContext) {
        let asset = Asset { id: object::new(ctx), value };
        let referent = borrow::new(asset, ctx);
        transfer::share_object(Vault { id: object::new(ctx), referent });
    }

    public fun borrow_and_return(vault: &mut Vault, new_value: u64) {
        let (mut asset, borrow) = borrow::borrow(&mut vault.referent);
        asset.value = new_value;
        borrow::put_back(&mut vault.referent, asset, borrow);
    }
}
