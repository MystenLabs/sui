// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises object receiving (`transfer::receive` / `public_receive` and
/// `ObjectArg::Receiving`). A single shared `Parent` is created at publish time;
/// items are transferred to its address and later received back. Using one shared
/// parent ensures the surfer reliably pairs a "send" with a later "receive".
module move_building_blocks::receiving {
    use sui::transfer::Receiving;

    public struct Parent has key {
        id: UID,
        sent: u64,
        received: u64,
    }

    public struct Item has key, store {
        id: UID,
        value: u64,
    }

    fun init(ctx: &mut TxContext) {
        let mut parent = Parent {
            id: object::new(ctx),
            sent: 0,
            received: 0,
        };
        // Seed some items already owned by the parent so the surfer can exercise
        // `receive` without first having to land a `send` transaction.
        let parent_address = parent.id.uid_to_address();
        let mut i = 0;
        while (i < 8) {
            transfer::transfer(Item { id: object::new(ctx), value: i }, parent_address);
            i = i + 1;
        };
        parent.sent = 8;
        transfer::share_object(parent);
    }

    /// Transfer a freshly created item to the parent object's address so it can
    /// later be received.
    public fun send_item_to_parent(parent: &mut Parent, value: u64, ctx: &mut TxContext) {
        let item = Item { id: object::new(ctx), value };
        transfer::transfer(item, parent.id.uid_to_address());
        parent.sent = parent.sent + 1;
    }

    /// Receive an item using the restricted (`receive`) variant and forward it to
    /// the sender.
    public fun receive_item(parent: &mut Parent, item: Receiving<Item>, ctx: &mut TxContext) {
        let received = transfer::receive(&mut parent.id, item);
        parent.received = parent.received + 1;
        transfer::transfer(received, ctx.sender());
    }

    /// Receive an item using the public (`public_receive`) variant and delete it.
    public fun receive_and_delete(parent: &mut Parent, item: Receiving<Item>) {
        let Item { id, value: _ } = transfer::public_receive(&mut parent.id, item);
        parent.received = parent.received + 1;
        object::delete(id);
    }
}
