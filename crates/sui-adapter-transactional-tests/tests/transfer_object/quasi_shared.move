// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests TransferObject should fail for a quasi-shared object

//# init --accounts A B --addresses test=0x0

//# publish

module test::m {
    use sui::transfer::{Self, ChildRef};
    use sui::tx_context::{Self, TxContext};
    use sui::id::VersionedID;

    struct S has key { id: VersionedID, children: vector<ChildRef<Child>> }
    struct Child has key { id: VersionedID }

    public entry fun mint_s(ctx: &mut TxContext) {
        let id = tx_context::new_id(ctx);
        transfer::share_object(S { id, children: vector[] })
    }

    public entry fun mint_child(s: &mut S, ctx: &mut TxContext) {
        let id = tx_context::new_id(ctx);
        let child = transfer::transfer_to_object(Child { id }, s);
        std::vector::push_back(&mut s.children, child)
    }
}

//# run test::m::mint_s

//# run test::m::mint_child --args object(107)

//# view-object 109

//# transfer-object 109 --sender A --recipient B

//# view-object 109
