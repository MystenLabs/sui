// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Transfer objects to not-yet-generated addresses with
// a nested derived path (parent -> derived_a -> derived_b).
// then claim using `derived_a` and `derived_b`.

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

entry fun setup(ctx: &mut TxContext) {
  let parent = Parent { id: object::new(ctx) };
  let derived_address_a = derived_object::derive_address(parent.id.to_inner(), 0u64);
  let derived_address_b = derived_object::derive_address(derived_address_a.to_id(), 0u64);
  transfer::public_transfer(parent, ctx.sender());
  transfer::public_transfer(Obj { id: object::new(ctx) }, derived_address_a);
  transfer::public_transfer(Obj { id: object::new(ctx) }, derived_address_b);
}

entry fun claim_and_receive_nested(
  parent: &mut Parent,
  receiving_a: Receiving<Obj>,
  receiving_b: Receiving<Obj>,
  ctx: &mut TxContext,
) {
  let mut derived_a = Derived { id: derived_object::claim(&mut parent.id, 0u64) };
  let mut derived_b = Derived { id: derived_object::claim(&mut derived_a.id, 0u64) };
  let obj_a = transfer::public_receive(&mut derived_a.id, receiving_a);
  let obj_b = transfer::public_receive(&mut derived_b.id, receiving_b);
  transfer::public_transfer(derived_a, ctx.sender());
  transfer::public_transfer(derived_b, ctx.sender());
  transfer::public_transfer(obj_a, ctx.sender());
  transfer::public_transfer(obj_b, ctx.sender());
}

//# run a::m::setup --sender A

//# run a::m::claim_and_receive_nested --sender A --args object(2,2) receiving(2,1) receiving(2,0)
