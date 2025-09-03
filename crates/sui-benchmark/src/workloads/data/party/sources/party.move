// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module party::party {
    use sui::party;

    public struct Obj has key {
        id: object::UID,
    }

    /// Create a single-owner party object and tranfer to the sender.
    public fun create_party(ctx: &mut TxContext) {
        transfer::party_transfer(
            Obj {
                id: object::new(ctx),
            },
            party::single_owner(ctx.sender()),
        );
    }

    /// Create a single-owner fastpath object and transfer to the sender.
    public fun create_fastpath(ctx: &mut TxContext) {
        transfer::transfer(
            Obj {
                id: object::new(ctx),
            },
            ctx.sender(),
        );
    }

    /// Transfer an object to a party owner.
    public fun transfer_party(obj: Obj, recipient: address) {
        transfer::party_transfer(obj, party::single_owner(recipient));
    }

    /// Transfer an object to a fastpath owner.
    public fun transfer_fastpath(obj: Obj, recipient: address) {
        transfer::transfer(obj, recipient);
    }
}
