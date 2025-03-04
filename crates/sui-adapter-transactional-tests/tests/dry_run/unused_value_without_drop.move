// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// dry run should trigger unused value without drop error

//# init --addresses test=0x0 --accounts A

//# publish
module test::m {
    public struct Object has key, store { id: UID }

    public fun return_object(ctx: &mut TxContext): Object {
        Object { id: object::new(ctx) }
    }
}

//# programmable --sender A --dry-run
//> 0: test::m::return_object();
