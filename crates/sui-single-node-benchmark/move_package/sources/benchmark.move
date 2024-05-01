// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_benchmark::benchmark {
    use std::ascii;
    use std::ascii::String;
    use sui::coin::Coin;
    use sui::dynamic_field;
    use sui::sui::SUI;

    #[allow(lint(self_transfer))]
    public fun transfer_coin(coin: Coin<SUI>, ctx: &TxContext) {
        transfer::public_transfer(coin, tx_context::sender(ctx));
    }

    // === compute-heavy workload ===

    public fun run_computation(mut num: u64) {
        // Store all numbers in an array to exercise memory consumption.
        let mut results = vector<u64>[];
        vector::push_back(&mut results, 1);
        vector::push_back(&mut results, 1);
        while (num > 0) {
            let len = vector::length(&results);
            let last = vector::borrow(&results, len - 1);
            let second_last = vector::borrow(&results, len - 2);
            let mut sum = *last + *second_last;
            if (sum >= 1_000_000_000_000_000_000u64) {
                sum = sum % 1_000_000_000_000_000_000u64;
            };
            vector::push_back(&mut results, sum);
            num = num - 1;
        }
    }

    // === dynamic field workload ===

    public struct RootObject has key {
        id: UID,
        child_count: u64,
    }

    public struct Child has store {
        field1: u64,
        field2: String,
    }

    public entry fun generate_dynamic_fields(num: u64, ctx: &mut TxContext) {
        let mut root = RootObject {
            id: object::new(ctx),
            child_count: num,
        };
        let mut i = 0;
        while (i < num) {
            let child = Child {
                field1: i,
                field2: ascii::string(b"a string"),
            };
            dynamic_field::add(&mut root.id, i, child);
            i = i + 1;
        };
        transfer::transfer(root, tx_context::sender(ctx));
    }

    public fun read_dynamic_fields(root: &RootObject) {
        let mut i = 0;
        while (i < root.child_count) {
            let child: &Child = dynamic_field::borrow(&root.id, i);
            assert!(child.field1 == i, 0);
            i = i + 1;
        }
    }

    // === shared object workload ===

    public struct SharedCounter has key {
        id: UID,
        count: u64,
    }

    public fun create_shared_counter(ctx: &mut TxContext) {
        let counter = SharedCounter {
            id: object::new(ctx),
            count: 0,
        };
        transfer::share_object(counter);
    }

    public fun increment_shared_counter(counter: &mut SharedCounter) {
        counter.count = counter.count + 1;
    }

    // === mint workload ===

    public struct NFT has key {
        id: UID,
        // mimic NFT's of arbitrary size
        contents: vector<u8>,
    }

    /// Create one NFT, send it to `recipient`
    public fun mint_one(recipient: address, contents: vector<u8>, ctx: &mut TxContext) {
        let nft = NFT { id: object::new(ctx), contents };
        transfer::transfer(nft, recipient)
    }

    /// Create one NFT, send it to each of the `recipients`
    public fun batch_mint(recipients: vector<address>, contents: vector<u8>, ctx: &mut TxContext) {
        let mut i = 0;
        let len = recipients.length();
        while (i < len) {
            let nft = NFT { id: object::new(ctx), contents };
            transfer::transfer(nft, recipients[i]);
            i = i + 1
        }
    }
}
