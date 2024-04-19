// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0

//# publish
module Test::f {
    public enum F has drop, store {
        V1,
        V2(u64),
        V3(u64, u64),
        V4 { x: u64 },
    }

    public struct S has key {
        id: UID,
        data: F,
    }

    public fun create_and_test(ctx: &mut TxContext) {
        let s = S {
            id: object::new(ctx),
            data: F::V1,
        };
        transfer::transfer(s, ctx.sender());
    }

    public fun update_inner(s: &mut S) {
        s.data = F::V4 { x: 42 };
    }
}

//# run Test::f::create_and_test

//# view-object 2,0

//# run Test::f::update_inner --args object(2,0)

//# view-object 2,0
