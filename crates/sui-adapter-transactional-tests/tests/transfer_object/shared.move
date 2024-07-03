// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests TransferObject should fail for a shared object with and without store

//# init --accounts A B --addresses test=0x0 --shared-object-deletion true

//# publish

module test::m {

    public struct S has key { id: UID }

    public struct S2 has key, store { id: UID }

    public fun mint_s(ctx: &mut TxContext) {
        let id = object::new(ctx);
        transfer::share_object(S { id })
    }

    public fun mint_s2(ctx: &mut TxContext) {
        let id = object::new(ctx);
        transfer::share_object(S2 { id })
    }
}

//# run test::m::mint_s

//# run test::m::mint_s2

//# view-object 2,0

//# view-object 3,0

//# transfer-object 2,0 --sender A --recipient B

//# transfer-object 3,0 --sender A --recipient B

//# view-object 2,0

//# view-object 3,0
