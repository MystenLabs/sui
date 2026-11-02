// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A parent object that receives children sent to its address. The parent can be
/// address-owned, shared, or party-owned.
module basics::receiver {
    use sui::party;
    use sui::transfer::Receiving;

    public struct Parent has key {
        id: UID,
        received: u64,
    }

    public struct Child has key, store {
        id: UID,
    }

    public fun create_owned_parent(recipient: address, ctx: &mut TxContext) {
        transfer::transfer(Parent { id: object::new(ctx), received: 0 }, recipient);
    }

    public fun create_shared_parent(ctx: &mut TxContext) {
        transfer::share_object(Parent { id: object::new(ctx), received: 0 });
    }

    public fun create_party_parent(recipient: address, ctx: &mut TxContext) {
        transfer::party_transfer(
            Parent { id: object::new(ctx), received: 0 },
            party::single_owner(recipient),
        );
    }

    /// Create a child owned by the object at `parent`.
    public fun create_child(parent: address, ctx: &mut TxContext) {
        transfer::public_transfer(Child { id: object::new(ctx) }, parent);
    }

    public fun send_child(c: Child, parent: address) {
        transfer::public_transfer(c, parent);
    }

    /// Receive a child and transfer it to `recipient`.
    public fun receive_child(p: &mut Parent, r: Receiving<Child>, recipient: address) {
        let c = transfer::public_receive(&mut p.id, r);
        p.received = p.received + 1;
        transfer::public_transfer(c, recipient);
    }

    public fun touch(p: &mut Parent) {
        p.received = p.received + 0;
    }
}
