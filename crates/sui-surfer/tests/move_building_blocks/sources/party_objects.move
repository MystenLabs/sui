// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises party objects (`ConsensusAddressOwner`) via `party_transfer` /
/// `public_party_transfer`. Objects are transferred to the sender as a single
/// owner party so the sender can later mutate, re-party, or delete them.
module move_building_blocks::party_objects {
    use sui::party;

    public struct PartyObject has key, store {
        id: UID,
        value: u64,
    }

    public fun create_to_sender(value: u64, ctx: &mut TxContext) {
        let obj = PartyObject { id: object::new(ctx), value };
        transfer::public_party_transfer(obj, party::single_owner(ctx.sender()));
    }

    public fun mutate(obj: &mut PartyObject, value: u64) {
        obj.value = value;
    }

    /// Re-transfer the existing party object to the sender as a party object. The
    /// input consensus version stays constant across re-transfers.
    public fun reparty(obj: PartyObject, ctx: &TxContext) {
        transfer::public_party_transfer(obj, party::single_owner(ctx.sender()));
    }

    public fun delete(obj: PartyObject) {
        let PartyObject { id, value: _ } = obj;
        object::delete(id);
    }
}
