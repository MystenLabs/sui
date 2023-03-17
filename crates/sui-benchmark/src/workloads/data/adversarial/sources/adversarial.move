// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module adversarial::adversarial {
    use std::vector;
    use sui::bcs;
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::event;
    use sui::dynamic_field::add;

    struct S has key, store {
        id: UID,
        contents: vector<u8>
    }

    struct Wrapper has key {
        id: UID,
        s: S,
    }

    // create an object whose Move BCS representation is `n` bytes
    public fun create_object_with_size(n: u64, ctx: &mut TxContext): S {
        // minimum object size for S is 32 bytes for UID + 1 byte for vector length
        assert!(n > std::address::length() + 1, 0);
        let contents = vector[];
        let i = 0;
        let bytes_to_add = n - (std::address::length() + 1);
        while (i < bytes_to_add) {
            vector::push_back(&mut contents, 9);
            i = i + 1;
        };
        let s = S { id: object::new(ctx), contents };
        let size = vector::length(&bcs::to_bytes(&s));
        // shrink by 1 byte until we match size. mismatch happens because of len(UID) + vector length byte
        while (size > n) {
            let _ = vector::pop_back(&mut s.contents);
            // hack: assume this doesn't change the size of the BCS length byte
            size = size - 1;
        };
        // double-check that we got the size right
        assert!(vector::length(&bcs::to_bytes(&s)) == n, 1);
        s
    }

    public fun create_max_size_object(size: u64, ctx: &mut TxContext): S {
        create_object_with_size(size, ctx)
    }

    /// Create `n` max size objects and transfer them to the tx sender
    public fun create_max_size_owned_objects(n: u64, size: u64, ctx: &mut TxContext) {
        let i = 0;
        let sender = tx_context::sender(ctx);
        while (i < n) {
            transfer::transfer(create_max_size_object(size, ctx), sender);
            i = i + 1
        }
    }

    struct NewValueEvent has copy, drop {
        contents: vector<u8>
    }

    // TODO: factor out the common bits with `create_object_with_size`
    // emit an event of size n
    public fun emit_event_with_size(n: u64) {
        // 46 seems to be the added size from event size derivation for `NewValueEvent`
        assert!(n > 46, 0);
        n = n - 46;
        // minimum object size for NewValueEvent is 1 byte for vector length
        assert!(n > 1, 0);
        let contents = vector[];
        let i = 0;
        let bytes_to_add = n - 1;
        while (i < bytes_to_add) {
            vector::push_back(&mut contents, 9);
            i = i + 1;
        };
        let s = NewValueEvent { contents };
        let size = vector::length(&bcs::to_bytes(&s));
        // shrink by 1 byte until we match size. mismatch happens because of len(UID) + vector length byte
        while (size > n) {
            let _ = vector::pop_back(&mut s.contents);
            // hack: assume this doesn't change the size of the BCS length byte
            size = size - 1;
        };

        event::emit(s);
    }

    struct Obj has key {
        id: object::UID,
    }

    public fun add_dynamic_fields(n: u64, ctx: &mut TxContext) {
        let i = 0;
        while (i < n) {
            let id = object::new(ctx);
            add<u64, u64>(&mut id, i, i);
            sui::transfer::transfer(Obj { id }, tx_context::sender(ctx));

            i = i + 1;
        };
    }


    /// Emit `n` events of size `size`
    public fun emit_events(n: u64, size: u64, _ctx: &mut TxContext) {
        let i = 0;
        while (i < n) {
            emit_event_with_size(size);
            i = i + 1
        }
    }

    /// Create `n` max size objects and share them
    public fun create_shared_objects(n: u64, size: u64, ctx: &mut TxContext) {
        let i = 0;
        while (i < n) {
            transfer::share_object(create_max_size_object(size, ctx));
            i = i + 1
        }
    }
}
