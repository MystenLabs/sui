// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module move_benchmark::benchmark {
    use std::ascii;
    use std::ascii::String;
    use std::vector;
    use sui::coin::Coin;
    use sui::dynamic_field;
    use sui::object;
    use sui::object::UID;
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context;
    use sui::tx_context::TxContext;

    public fun transfer_coin(coin: Coin<SUI>, ctx: &TxContext) {
        transfer::public_transfer(coin, tx_context::sender(ctx));
    }

    public fun run_computation(num: u64) {
        // Store all numbers in an array to exercise memory consumption.
        let results = vector<u64>[];
        vector::push_back(&mut results, 1);
        vector::push_back(&mut results, 1);
        while (num > 0) {
            let len = vector::length(&results);
            let last = vector::borrow(&results, len - 1);
            let second_last = vector::borrow(&results, len - 2);
            let sum = *last + *second_last;
            if (sum >= 1_000_000_000_000_000_000u64) {
                sum = sum % 1_000_000_000_000_000_000u64;
            };
            vector::push_back(&mut results, sum);
            num = num - 1;
        }
    }

    struct RootObject has key {
        id: UID,
        child_count: u64,
    }

    struct Child has store {
        field1: u64,
        field2: String,
    }

    public entry fun generate_dynamic_fields(num: u64, ctx: &mut TxContext) {
        let root = RootObject {
            id: object::new(ctx),
            child_count: num,
        };
        let i = 0;
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
        let i = 0;
        while (i < root.child_count) {
            let child: &Child = dynamic_field::borrow(&root.id, i);
            assert!(child.field1 == i, 0);
            i = i + 1;
        }
    }
}
