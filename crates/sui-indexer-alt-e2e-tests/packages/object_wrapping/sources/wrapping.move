// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module object_wrapping::wrapping {
    public struct Item has key, store {
        id: UID,
        value: u64,
    }

    public struct Wrapper has key, store {
        id: UID,
        item: Item,
    }

    public fun create(value: u64, ctx: &mut TxContext): Item {
        Item { id: object::new(ctx), value }
    }

    public fun update(item: &mut Item, value: u64) {
        item.value = value;
    }

    public fun wrap(item: Item, ctx: &mut TxContext): Wrapper {
        Wrapper { id: object::new(ctx), item }
    }

    public fun unwrap(wrapper: Wrapper): Item {
        let Wrapper { id, item } = wrapper;
        id.delete();
        item
    }
}
