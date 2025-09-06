// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module b::m {
    const COUNT: u64 = 10_000u64;

    public fun bench_call_empty_xmodule_function(): u64 {
        let i : u64 = 0;
        while (i < COUNT) {
            i = i + a::m::empty_function_pub();
        };
        i
    }
}
