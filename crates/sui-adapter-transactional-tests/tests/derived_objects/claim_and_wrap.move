// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Claim a derived UID and wrap it in its parent.

//# init --addresses a=0x0 --accounts A

//# publish
module a::m;

use sui::derived_object;
use sui::transfer::Receiving;

public struct Parent has key, store {
  id: UID,
  wrapped: Option<UID>,
}

public struct Obj has key, store {
  id: UID,
}

entry fun create_parent(ctx: &mut TxContext) {
  transfer::public_transfer(Parent { id: object::new(ctx), wrapped: option::none() }, ctx.sender());
}

entry fun claim_and_wrap(parent: &mut Parent, key: u64) {
  parent.wrapped.fill(derived_object::claim(&mut parent.id, key));
}

entry fun transfer_to_wrapped(parent: &Parent, key: u64, ctx: &mut TxContext) {
  let obj = Obj { id: object::new(ctx) };
  let recipient = derived_object::derive_address(parent.id.to_inner(), key);
  transfer::transfer(obj, recipient);
}

entry fun receive_from_derived_wrapped(
  parent: &mut Parent,
  receiving: Receiving<Obj>,
  ctx: &mut TxContext,
) {
  let obj = transfer::public_receive(parent.wrapped.borrow_mut(), receiving);
  transfer::public_transfer(obj, ctx.sender());
}

entry fun claim_and_wrap_and_receive(
  parent: &mut Parent,
  key: u64,
  receiving: Receiving<Obj>,
  ctx: &mut TxContext,
) {
  claim_and_wrap(parent, key);
  let obj = transfer::public_receive(parent.wrapped.borrow_mut(), receiving);
  transfer::public_transfer(obj, ctx.sender());
}

//# run a::m::create_parent --sender A

//# run a::m::claim_and_wrap --sender A --args object(2,0) 0

//# view-object 2,0

//# view-object 3,0

//# run a::m::transfer_to_wrapped --sender A --args object(2,0) 0

//# run a::m::receive_from_derived_wrapped --sender A --args object(2,0) receiving(6,0)

//# run a::m::create_parent --sender A

//# run a::m::transfer_to_wrapped --sender A --args object(8,0) 1

//# run a::m::claim_and_wrap_and_receive --sender A --args object(8,0) 1 receiving(9,0)
