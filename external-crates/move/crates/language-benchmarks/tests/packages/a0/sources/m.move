// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
module a::m {
    const COUNT: u64 = 10_000u64;
    public fun empty_function_pub(): u64 {
        1
    }

    public fun empty_function(): u64 {
        1
    }

    public fun bench_call_empty_internal_function(): u64 {
        let i : u64 = 0;
        while (i < COUNT) {
            i = i + empty_function();
        };
        i
    }

    public fun bench_call_empty_pub_function(): u64 {
        let i : u64 = 0;
        while (i < COUNT) {
            i = i + empty_function_pub();
        };
        i
    }
}
