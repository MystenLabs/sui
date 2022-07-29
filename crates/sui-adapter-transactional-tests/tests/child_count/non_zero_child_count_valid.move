// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests valid transfers of an object that has children

//# init --addresses test=0x0 --accounts A B

//# publish

module test::m {
    use sui::tx_context::{Self, TxContext};

    struct S has key, store {
        id: sui::object::UID,
    }

    public entry fun mint(ctx: &mut TxContext) {
        let s = S { id: sui::object::new(ctx) };
        sui::transfer::transfer(s, tx_context::sender(ctx))
    }

    public entry fun share(s: S) {
        sui::transfer::share_object(s)
    }

    public entry fun transfer(s: S, recipient: address) {
        sui::transfer::transfer(s, recipient)
    }

    public entry fun transfer_to_object(child: S, parent: &mut S) {
        sui::transfer::transfer_to_object(child, parent)
    }

}

//
// Test transfer_to_object allows non-zero child count
//

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# run test::m::transfer_to_object --sender A --args object(111) object(109)

//# view-object 109

//# run test::m::transfer_to_object --sender A --args object(109) object(107)

//
// Test share object allows non-zero child count
//

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# run test::m::transfer_to_object --sender A --args object(117) object(115)

//# view-object 115

//# run test::m::share --sender A --args object(115)

//
// Test transfer allows non-zero child count
//

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# run test::m::transfer_to_object --sender A --args object(123) object(121)

//# view-object 121

//# run test::m::transfer --sender A --args object(121) @B

//
// Test TransferObject allows non-zero child count
//

//# run test::m::mint --sender A

//# run test::m::mint --sender A

//# run test::m::transfer_to_object --sender A --args object(129) object(127)

//# view-object 127

//# transfer-object 127 --sender A --recipient B
