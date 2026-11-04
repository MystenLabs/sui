// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The parent of a derived object is itself derived. Releasing and re-claiming the parent while its
// child is live keeps the child's claim, so the child cannot be claimed a second time.

//# init --addresses ex=0x0 --accounts A

//# publish
module ex::m;

use sui::derived_object;

public struct Root has key { id: UID }

public struct Parent has key, store { id: UID }

public struct Child has key, store { id: UID, value: u64 }

public fun create_root(ctx: &mut TxContext) {
    transfer::transfer(Root { id: object::new(ctx) }, ctx.sender())
}

public fun claim_parent(root: &mut Root): Parent {
    Parent { id: derived_object::claim(&mut root.id, 0u64) }
}

public fun release_parent(root: &mut Root, parent: Parent) {
    let Parent { id } = parent;
    derived_object::release(&mut root.id, id)
}

public fun claim_child(parent: &mut Parent): Child {
    Child { id: derived_object::claim(&mut parent.id, 1u64), value: 0 }
}

public fun release_child(parent: &mut Parent, child: Child) {
    let Child { id, value: _ } = child;
    derived_object::release(&mut parent.id, id)
}

public fun assert_child_claimed(parent: &Parent, claimed: bool) {
    assert!(derived_object::exists(&parent.id, 1u64) == claimed)
}

public fun set(child: &mut Child, value: u64) {
    child.value = value
}

//# run ex::m::create_root --sender A

//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim_parent(Input(0));
//> 1: ex::m::claim_child(Result(0));
//> TransferObjects([Result(0), Result(1)], Input(1))

//# view-object 3,1

//# view-object 3,0

// Release the child, release the parent, re-claim the parent, re-claim the child.
//# programmable --sender A --inputs object(2,0) object(3,1) object(3,0) @A
//> ex::m::release_child(Input(1), Input(2));
//> ex::m::release_parent(Input(0), Input(1));
//> 2: ex::m::claim_parent(Input(0));
//> 3: ex::m::claim_child(Result(2));
//> TransferObjects([Result(2), Result(3)], Input(3))

// Release the parent while the child is live.
//# programmable --sender A --inputs object(2,0) object(3,1)
//> ex::m::release_parent(Input(0), Input(1))

// The re-claimed parent still has the child's claim.
//# programmable --sender A --inputs object(2,0) true @A
//> 0: ex::m::claim_parent(Input(0));
//> ex::m::assert_child_claimed(Result(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(2,0) object(3,1)
//> ex::m::release_parent(Input(0), Input(1))

// Claiming the live child again aborts with EObjectAlreadyExists.
//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim_parent(Input(0));
//> 1: ex::m::claim_child(Result(0));
//> TransferObjects([Result(0), Result(1)], Input(1))

//# programmable --sender A --inputs object(2,0) @A
//> 0: ex::m::claim_parent(Input(0));
//> TransferObjects([Result(0)], Input(1))

// The same within one transaction: release and re-claim the parent, then try to claim the child.
//# programmable --sender A --inputs object(2,0) object(3,1) true @A
//> ex::m::release_parent(Input(0), Input(1));
//> 1: ex::m::claim_parent(Input(0));
//> ex::m::assert_child_claimed(Result(1), Input(2));
//> TransferObjects([Result(1)], Input(3))

// Release and re-claim the parent, then claim the live child, in one transaction: aborts with
// EObjectAlreadyExists.
//# programmable --sender A --inputs object(2,0) object(3,1) @A
//> ex::m::release_parent(Input(0), Input(1));
//> 1: ex::m::claim_parent(Input(0));
//> 2: ex::m::claim_child(Result(1));
//> TransferObjects([Result(1), Result(2)], Input(2))

// The original child is still usable.
//# run ex::m::set --sender A --args object(3,0) 7

//# view-object 3,0
