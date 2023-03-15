// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module adversarial::adversarial {
    use std::vector;
    use sui::bcs;
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

    struct S has key, store {
        id: UID,
        contents: vector<u8>
    }

    struct Wrapper has key {
        id: UID,
        s: S,
    }

    const MAX_OBJ_SIZE: u64 = 25600;

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

    public fun create_max_size_object(ctx: &mut TxContext): S {
        create_object_with_size(MAX_OBJ_SIZE, ctx)
    }

    /// Create `n` max size objects and transfer them to the tx sender
    public fun create_max_size_owned_objects(n: u64, ctx: &mut TxContext) {
        let i = 0;
        let sender = tx_context::sender(ctx);
        while (i < n) {
            transfer::transfer(create_max_size_object(ctx), sender);
            i = i + 1
        }
    }

    /// Create `n` max size objects and share them
    public fun create_max_size_shared_objects(n: u64, ctx: &mut TxContext) {
        let i = 0;
        while (i < n) {
            transfer::share_object(create_max_size_object(ctx));
            i = i + 1
        }
    }
}
