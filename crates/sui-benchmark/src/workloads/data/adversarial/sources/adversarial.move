// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module adversarial::adversarial {
    use std::vector;
    use sui::bcs;
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;
    use sui::event;
    use sui::dynamic_field::{add, borrow};

    const NUM_DYNAMIC_FIELDS: u64 = 33;

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

    /// Create `n` owned objects of size `size` and transfer them to the tx sender
    public fun create_owned_objects(n: u64, size: u64, ctx: &mut TxContext) {
        let i = 0;
        let sender = tx_context::sender(ctx);
        while (i < n) {
            transfer::public_transfer(create_object_with_size(size, ctx), sender);
            i = i + 1
        }
    }

    struct NewValueEvent has copy, drop {
        contents: vector<u8>
    }

    // TODO: factor out the common bits with `create_object_with_size`
    // emit an event of size n
    public fun emit_event_with_size(n: u64) {
        // 55 seems to be the added size from event size derivation for `NewValueEvent`
        assert!(n > 55, 0);
        n = n - 55;
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

    struct Obj has key, store {
        id: object::UID,
    }

    public fun add_dynamic_fields(obj: &mut Obj, n: u64) {
        let i = 0;
        while (i < n) {
            add<u64, u64>(&mut obj.id, i, i);
            i = i + 1;
        };
    }

    public fun read_n_dynamic_fields(obj: &mut Obj, n: u64) {
        let i = 0;
        while (i < n) {
            let _ = borrow<u64, u64>(&obj.id, i);
            i = i + 1;
        };
    }

    /// Emit `n` events of size `size`
    public fun emit_events(n: u64, size: u64) {
        let i = 0;
        while (i < n) {
            emit_event_with_size(size);
            i = i + 1
        }
    }

    /// Create `n` objects of size `size` and share them
    public fun create_shared_objects(n: u64, size: u64, ctx: &mut TxContext) {
        let i = 0;
        while (i < n) {
            transfer::public_share_object(create_object_with_size(size, ctx));
            i = i + 1
        }
    }

    /// Initialize object to be used for dynamic field opers
    fun init(ctx: &mut TxContext) {
        let id = object::new(ctx);
        let x = Obj { id };
        add_dynamic_fields(&mut x, NUM_DYNAMIC_FIELDS);
        transfer::share_object(x);
    }
}
