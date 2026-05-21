// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises derived objects (`derived_object::claim` / `exists` /
/// `derive_address`).
module move_building_blocks::derived_objects {
    use sui::derived_object;

    public struct Registry has key, store {
        id: UID,
    }

    public struct Derived has key, store {
        id: UID,
        key: u64,
    }

    public fun create_registry(ctx: &mut TxContext) {
        transfer::share_object(Registry { id: object::new(ctx) });
    }

    public fun claim(registry: &mut Registry, key: u64, ctx: &mut TxContext) {
        if (!derived_object::exists(&registry.id, key)) {
            let uid = derived_object::claim(&mut registry.id, key);
            transfer::transfer(Derived { id: uid, key }, ctx.sender());
        }
    }
}
