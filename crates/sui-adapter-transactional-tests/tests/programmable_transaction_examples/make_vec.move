// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use std::vector;
    use std::string::{Self, String};
    use std::option::{Self, Option};

    struct CoolMarker has key, store { id: UID }

    public entry fun vec_u64(_: vector<u64>) {
    }

    public entry fun vec_vec_u64(_: vector<vector<u64>>) {
    }

    public entry fun vec_string(v: vector<String>) {
        while (!vector::is_empty(&v)) {
            string::utf8(*string::bytes(&vector::pop_back(&mut v)));
        }
    }

    public entry fun vec_vec_string(v: vector<vector<String>>) {
        while (!vector::is_empty(&v)) vec_string(vector::pop_back(&mut v))
    }

    public entry fun vec_option_string(v: vector<Option<String>>) {
        while (!vector::is_empty(&v)) {
            let opt = vector::pop_back(&mut v);
            if (option::is_some(&opt)) {
                string::utf8(*string::bytes(&option::destroy_some(opt)));
            }
        }
    }

    public fun marker(ctx: &mut TxContext): CoolMarker {
        CoolMarker { id: object::new(ctx) }
    }

    public fun burn_markers(markers: vector<CoolMarker>) {
        while (!vector::is_empty(&markers)) {
            let CoolMarker { id } = vector::pop_back(&mut markers);
            object::delete(id);
        };
        vector::destroy_empty(markers);
    }

}

//# programmable --inputs 112u64
// vector<u64>
//> 0: MakeMoveVec<u64>([Input(0), Input(0)]);
//> 1: test::m1::vec_u64(Result(0));
// vector<vector<u64>>
//> 2: MakeMoveVec<vector<u64>>([Result(0), Result(0)]);
//> 3: test::m1::vec_vec_u64(Result(2));

//# programmable --inputs vector[226u8,157u8,164u8,239u8,184u8,143u8]
// vector<String>
//> 0: MakeMoveVec<std::string::String>([Input(0), Input(0)]);
//> 1: test::m1::vec_string(Result(0));
// vector<vector<String>>
//> 2: MakeMoveVec<vector<std::string::String>>([Result(0), Result(0)]);
//> 3: test::m1::vec_vec_string(Result(2));

//# programmable --inputs vector[vector[226u8,157u8,164u8,239u8,184u8,143u8]] vector[]
// Option<String>
//> 0: MakeMoveVec<std::option::Option<std::string::String>>([Input(0), Input(1)]);
//> 1: test::m1::vec_option_string(Result(0));

//# programmable --inputs vector[255u8,157u8,164u8,239u8,184u8,143u8]
// INVALID string                ^^^ modified the bytes to make an invalid UTF8 string
//> 0: MakeMoveVec<std::string::String>([Input(0), Input(0)]);
//> 1: test::m1::vec_string(Result(0));

//# programmable --sender A
//> 0: test::m1::marker();
//> 1: test::m1::marker();
//> 2: MakeMoveVec([Result(0), Result(1)]);
//> 3: test::m1::burn_markers(Result(2));
