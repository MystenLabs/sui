// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test creating objects just below the size limit, and above it

//# init --addresses Test=0x0 --accounts A --max-gas 100000000000000

//# publish

module Test::M1 {
    use sui::bcs;

    public struct S has key, store {
        id: UID,
        contents: vector<u8>
    }

    public struct Wrapper has key {
        id: UID,
        s: S,
    }

    // create an object whose Move BCS representation is `n` bytes
    public fun create_object_with_size(n: u64, ctx: &mut TxContext): S {
        // minimum object size for S is 32 bytes for UID + 1 byte for vector length
        assert!(n > std::address::length() + 1, 0);
        let mut contents = vector[];
        let mut i = 0;
        let bytes_to_add = n - (std::address::length() + 1);
        while (i < bytes_to_add) {
            vector::push_back(&mut contents, 9);
            i = i + 1;
        };
        let mut s = S { id: object::new(ctx), contents };
        let mut size = vector::length(&bcs::to_bytes(&s));
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

    public entry fun transfer_object_with_size(n: u64, ctx: &mut TxContext) {
        transfer::public_transfer(create_object_with_size(n, ctx), tx_context::sender(ctx))
    }

    /// Add a byte to `s`
    public entry fun add_byte(s: &mut S) {
        vector::push_back(&mut s.contents, 9)
    }

    /// Wrap `s`
    public entry fun wrap(s: S, ctx: &mut TxContext) {
        transfer::transfer(Wrapper { id: object::new(ctx), s }, tx_context::sender(ctx))
    }

    /// Add `n` bytes to the `s` inside `wrapper`, then unwrap it. This should fail
    /// if `s` is larger than the max object size
    public entry fun add_bytes_then_unwrap(mut wrapper: Wrapper, n: u64, ctx: &mut TxContext) {
        let mut i = 0;
        while (i < n) {
            vector::push_back(&mut wrapper.s.contents, 7);
            i = i + 1
        };
        let Wrapper { id, s } = wrapper;
        object::delete(id);
        transfer::public_transfer(s, tx_context::sender(ctx))
    }
}


// tests below all run out of gas with realistic prices

// create above size limit should fail
//# run Test::M1::transfer_object_with_size --args 256001 --sender A --gas-budget 10000000000000

// create under size limit should succeed
//# run Test::M1::transfer_object_with_size --args 255999 --sender A --gas-budget 100000000000000

// create at size limit should succeed
//# run Test::M1::transfer_object_with_size --args 256000 --sender A --gas-budget 100000000000000
