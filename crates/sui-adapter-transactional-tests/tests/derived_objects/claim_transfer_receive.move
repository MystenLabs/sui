// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Claim a derived object, transfer an object to it, 
// receive it with the derived object.

//# init --addresses a=0x0 --accounts A

//# publish
module a::m;

use sui::derived_object;
use sui::transfer::Receiving;

public struct Parent has key, store {
  id: UID,
}

public struct Derived has key, store {
  id: UID,
}

public struct Obj has key, store {
  id: UID,
}

entry fun create_parent_and_derived(key: u64, ctx: &mut TxContext) {
  let mut parent = Parent { id: object::new(ctx) };
  let derived = Derived { id: derived_object::claim(&mut parent.id, key) };
  transfer::public_transfer(derived, ctx.sender());
  transfer::public_transfer(parent, ctx.sender());
}

entry fun transfer_obj_to_derived(parent: &Parent, key: u64, ctx: &mut TxContext) {
  let obj = Obj { id: object::new(ctx) };
  let recipient = derived_object::derive_address(parent.id.to_inner(), key);
  transfer::public_transfer(obj, recipient);
}

entry fun receive_obj_from_derived(
  derived: &mut Derived,
  receiving: Receiving<Obj>,
  ctx: &mut TxContext,
) {
  let obj = transfer::public_receive(&mut derived.id, receiving);
  transfer::public_transfer(obj, ctx.sender());
}

//# run a::m::create_parent_and_derived --args 0 --sender A

//# run a::m::transfer_obj_to_derived --sender A --args object(2,1) 0

//# view-object 3,0

//# run a::m::receive_obj_from_derived --sender A --args object(2,0) receiving(3,0)

//# view-object 3,0
