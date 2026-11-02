// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A party (ConsensusAddressOwner) object whose consensus stream can be ended by
/// transferring it to a plain address and later restarted by transferring it back to
/// a party owner. Used to stress the interaction between old-stream consensus
/// transactions and owned-object mutations of the same object id.
module basics::party_obj {
    use sui::party;

    public struct PartyObj has key, store {
        id: UID,
        value: u64,
    }

    /// Create a party object owned by `recipient` as a single owner.
    public fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        let o = PartyObj {
            id: object::new(ctx),
            value,
        };
        transfer::public_party_transfer(o, party::single_owner(recipient));
    }

    /// End the object's consensus stream by moving it to a plain address.
    public fun to_address(o: PartyObj, recipient: address) {
        transfer::public_transfer(o, recipient);
    }

    /// Start a new consensus stream by moving the object back to a party owner.
    public fun to_party(o: PartyObj, recipient: address) {
        transfer::public_party_transfer(o, party::single_owner(recipient));
    }

    public fun set_value(o: &mut PartyObj, value: u64) {
        o.value = value;
    }

    public fun value(o: &PartyObj): u64 {
        o.value
    }

    /// An address-owned object that wraps a party object, which ends the party object's
    /// consensus stream.
    public struct Holder has key {
        id: UID,
        inner: PartyObj,
    }

    public fun wrap(o: PartyObj, recipient: address, ctx: &mut TxContext) {
        transfer::transfer(Holder { id: object::new(ctx), inner: o }, recipient);
    }

    /// Unwrap the object onto a new consensus stream.
    public fun unwrap_to_party(h: Holder, recipient: address) {
        let Holder { id, inner } = h;
        id.delete();
        transfer::public_party_transfer(inner, party::single_owner(recipient));
    }

    /// Unwrap the object to a plain address.
    public fun unwrap_to_address(h: Holder, recipient: address) {
        let Holder { id, inner } = h;
        id.delete();
        transfer::public_transfer(inner, recipient);
    }

    public fun holder_value(h: &Holder): u64 {
        h.inner.value
    }
}
