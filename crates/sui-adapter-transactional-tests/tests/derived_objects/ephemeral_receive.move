// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Transfer an object to a derived address ahead of its creation,
// then claim the object and delete the claimed uid (ephemeral uid).

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

entry fun create_parent(ctx: &mut TxContext) {
  let parent = Parent { id: object::new(ctx) };
  transfer::public_transfer(parent, ctx.sender());
}

entry fun transfer_obj_to_derived(parent: &Parent, key: u64, ctx: &mut TxContext) {
  let obj = Obj { id: object::new(ctx) };
  let recipient = derived_object::derive_address(parent.id.to_inner(), key);
  transfer::public_transfer(obj, recipient);
}

entry fun ephemeral_receive(
  parent: &mut Parent,
  key: u64,
  receiving: Receiving<Obj>,
  ctx: &mut TxContext,
) {
  let mut uid = derived_object::claim(&mut parent.id, key);
  let obj = transfer::public_receive(&mut uid, receiving);
  uid.delete();
  transfer::public_transfer(obj, ctx.sender());
}

//# run a::m::create_parent --sender A

//# run a::m::transfer_obj_to_derived --sender A --args object(2,0) 0

//# run a::m::ephemeral_receive --sender A --args object(2,0) 0 receiving(3,0)
