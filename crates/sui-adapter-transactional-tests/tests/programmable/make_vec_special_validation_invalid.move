// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that MakeMoveVec performs necessary validation for special types like Option or String

//# init --addresses test=0x0 --accounts A

//# publish
module test::m1 {
    use std::string::{Self, String};

    public entry fun vec_option_u64(mut v: vector<Option<u64>>) {
        while (!vector::is_empty(&v)) {
            let opt = vector::pop_back(&mut v);
            if (option::is_some(&opt)) {
                option::destroy_some(opt);
            }
        }
    }

    public entry fun vec_option_string(mut v: vector<Option<String>>) {
        while (!vector::is_empty(&v)) {
            let opt = vector::pop_back(&mut v);
            if (option::is_some(&opt)) {
                string::utf8(*string::as_bytes(&option::destroy_some(opt)));
            }
        }
    }
}

//# programmable --inputs vector[0u64,0u64]
// INVALID option, using a vetor of length 2
//> 0: MakeMoveVec<std::option::Option<u64>>([Input(0)]);
//> 1: test::m1::vec_option_u64(Result(0));

//# programmable --inputs vector[255u8,157u8,164u8,239u8,184u8,143u8]
// INVALID string                ^^^ modified the bytes to make an invalid UTF8 string
//> 0: MakeMoveVec<std::string::String>([Input(0), Input(0)]);
//> 1: test::m1::vec_string(Result(0));
