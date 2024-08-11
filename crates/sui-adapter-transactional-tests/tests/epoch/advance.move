// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0

//# publish

module test::m {
    public struct S has key, store {
        id: UID,
        value: u64
    }

    public fun create(ctx: &mut TxContext) {
        transfer::public_share_object(S {
            id: object::new(ctx),
            value: ctx.epoch(),
        })
    }

    public fun check(s: &S, epoch_diff: u64, ctx: &TxContext) {
        assert!(s.value + epoch_diff == ctx.epoch());
    }
}

//# run test::m::create

//# run test::m::check --args object(2,0) 0

//# advance-epoch

//# run test::m::check --args object(2,0) 1
