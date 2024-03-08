// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// test old behavior of invariant violation

//# init --addresses test=0x0

//# publish
module test::m {
    public fun t1(cond: bool) {
        let mut x: vector<u64>;
        let r: &vector<u64>;
        if (cond) {
            x = vector[];
            r = &x;
            // use r in ways to disable optimizations or moving
            id_ref(r);
            id_ref(copy r);
        };
        x = vector[];
        // use x in ways to disable optimizations or moving
        id(x);
        id(x);
        return
    }

    public fun t2(cond: bool) {
        let x: vector<u64> = vector[];
        let r: &vector<u64>;
        if (cond) {
            r = &x;
            // use r in ways to disable optimizations or moving
            id_ref(r);
            id_ref(copy r);
        };
        _ = move x;
        return
    }

    fun id<T>(x: T): T { x }
    fun id_ref<T>(x: &T): &T { x }
}

//# run test::m::t1 --args true

//# run test::m::t2 --args true
