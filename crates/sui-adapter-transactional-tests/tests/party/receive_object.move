// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A B --addresses ex=0x0

//# publish
module ex::m;

public struct PubA has key, store {
    id: UID,
}

public struct PubB has key, store {
    id: UID,
}

public fun mint(ctx: &mut TxContext) {
    let fastpath_parent = PubA { id: object::new(ctx) };
    let fastpath_address = object::id_address(&fastpath_parent);
    let party_parent = PubA { id: object::new(ctx) };
    let party_address = object::id_address(&party_parent);

    transfer::public_transfer(fastpath_parent, tx_context::sender(ctx));
    transfer::public_party_transfer(party_parent, sui::party::single_owner(tx_context::sender(ctx)));

    let fastpath_child_fastpath_parent = PubB { id: object::new(ctx) };
    let fastpath_child_party_parent = PubB { id: object::new(ctx) };

    transfer::public_transfer(fastpath_child_fastpath_parent, fastpath_address);
    transfer::public_party_transfer(fastpath_child_party_parent, sui::party::single_owner(fastpath_address));

    let party_child_fastpath_parent = PubB { id: object::new(ctx) };
    let party_child_party_parent = PubB { id: object::new(ctx) };

    transfer::public_transfer(party_child_fastpath_parent, party_address);
    transfer::public_party_transfer(party_child_party_parent, sui::party::single_owner(party_address));
}

public entry fun receiver(parent: &mut PubA, x: sui::transfer::Receiving<PubB>) {
    let b = transfer::receive(&mut parent.id, x);
    transfer::public_transfer(b, @ex);
}

//# run ex::m::mint

// fastpath_parent
//# view-object 2,0

// party_parent
//# view-object 2,1

// fastpath_child_fastpath_parent
//# view-object 2,2

// fastpath_child_party_parent
//# view-object 2,3

// party_child_party_parent
//# view-object 2,4

// party_child_fastpath_parent
//# view-object 2,5


// 1. Can receive a fastpath object from a fastpath parent.
//# run ex::m::receiver --args object(2,0) receiving(2,2)

//# view-object 2,2


// 2. Can receive a fastpath object from a party parent.
//# run ex::m::receiver --args object(2,1) receiving(2,3)

//# view-object 2,3


// 3. Cannot receive a party object from any parent type.

//# run ex::m::receiver --args object(2,0) receiving(2,5)

//# run ex::m::receiver --args object(2,1) receiving(2,4)
