// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Claim a derived object & add a DF with the exact same key for the parent.
// We should have 4 objects created (parent, derived object, df, marker df)

//# init --addresses a=0x0 --accounts A

//# publish
module a::m;

use sui::derived_object;
use sui::dynamic_field;

public struct Parent has key, store {
  id: UID,
}

public struct Derived has key, store {
  id: UID,
}

entry fun create_and_claim(ctx: &mut TxContext) {
  let mut parent = Parent { id: object::new(ctx) };
  let mut derived = Derived { id: derived_object::claim(&mut parent.id, 0u64) };
  dynamic_field::add(&mut derived.id, 0u64, b"");
  transfer::public_transfer(parent, ctx.sender());
  transfer::public_transfer(derived, ctx.sender());
}

//# run a::m::create_and_claim --sender A

//# view-object 2,0

//# view-object 2,1

//# view-object 2,2

//# view-object 2,3
