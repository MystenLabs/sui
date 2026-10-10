// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Objects and the ways a package can take them out of their owner's possession, for exercising
/// account policy custody rules.
module move_test_code_account_policy::items;

public struct Item has key, store {
    id: UID,
}

public struct Wrapper has key, store {
    id: UID,
    item: Item,
}

public fun mint(ctx: &mut TxContext): Item {
    Item { id: object::new(ctx) }
}

public fun wrap(item: Item, ctx: &mut TxContext) {
    transfer::public_transfer(Wrapper { id: object::new(ctx), item }, ctx.sender())
}

public fun burn(item: Item) {
    let Item { id } = item;
    id.delete()
}

public fun send(item: Item, to: address) {
    transfer::public_transfer(item, to)
}
