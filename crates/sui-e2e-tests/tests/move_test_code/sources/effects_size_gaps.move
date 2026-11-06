// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Produces effects entries that are not written or modified objects: objects created then
/// wrapped, objects unwrapped then deleted, and read-only shared inputs.
module move_test_code::effects_size_gaps;

public struct Item has store {
    id: UID,
}

public struct Holder has key {
    id: UID,
    items: vector<Item>,
}

public struct Marker has key {
    id: UID,
}

public entry fun create_holder(count: u64, ctx: &mut TxContext) {
    let mut items = vector[];
    count.do!(|_| items.push_back(Item { id: object::new(ctx) }));
    transfer::transfer(Holder { id: object::new(ctx), items }, ctx.sender());
}

/// Each deleted item was wrapped before this transaction, so its effects entry is
/// unwrapped-then-deleted.
public entry fun delete_items(holder: &mut Holder) {
    while (!holder.items.is_empty()) {
        let Item { id } = holder.items.pop_back();
        id.delete();
    }
}

public entry fun create_markers(count: u64, ctx: &mut TxContext) {
    count.do!(|_| transfer::share_object(Marker { id: object::new(ctx) }));
}
