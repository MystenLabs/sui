// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests TransferObject should fail for a quasi-shared object

//# init --accounts A B --addresses test=0x0

//# publish

module test::m {
    public struct S has key { id: UID }
    public struct Child has key, store { id: UID }

    public entry fun mint_s(ctx: &mut TxContext) {
        let id = object::new(ctx);
        transfer::share_object(S { id })
    }

    public entry fun mint_child(s: &mut S, ctx: &mut TxContext) {
        let id = object::new(ctx);
        sui::dynamic_object_field::add(&mut s.id, 0, Child { id });
    }
}

//# run test::m::mint_s

//# run test::m::mint_child --args object(2,0)

//# view-object 3,0

//# transfer-object 3,0 --sender A --recipient B

//# view-object 3,0
