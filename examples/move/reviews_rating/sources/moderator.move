// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module reviews_rating::moderator {
    use sui::tx_context::{sender};

    /// Represents a moderator that can be used to delete reviews
    public struct Moderator has key {
        id: UID,
    }

    /// A capability that can be used to setup moderators
    public struct ModCap has key, store {
        id: UID
    }

    fun init(ctx: &mut TxContext) {
        let mod_cap = ModCap {
            id: object::new(ctx)
        };
        transfer::transfer(mod_cap, sender(ctx));
    }

    /// Adds a moderator
    public fun add_moderator(
        _: &ModCap,
        recipient: address,
        ctx: &mut TxContext
    ) {
        // generate an NFT and transfer it to moderator who may use it to delete reviews
        let mod = Moderator {
            id: object::new(ctx)
        };
        transfer::transfer(mod, recipient);
    }

    /// Deletes a moderator
    public fun delete_moderator(
        mod: Moderator
    ) {
        let Moderator { id } = mod;
        object::delete(id);
    }
}
