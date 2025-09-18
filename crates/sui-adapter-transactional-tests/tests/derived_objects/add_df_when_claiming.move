// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// When claiming a derived object, add a DF to it immediately.

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

entry fun create_parent(ctx: &mut TxContext) {
  transfer::public_transfer(Parent { id: object::new(ctx) }, ctx.sender());
}

entry fun derive_and_add_df(parent: &mut Parent, key: u64, ctx: &mut TxContext) {
  let mut id = derived_object::claim(&mut parent.id, key);
  dynamic_field::add(&mut id, b"key", b"value");
  transfer::public_transfer(Derived { id }, ctx.sender());
}

//# run a::m::create_parent --sender A

//# run a::m::derive_and_add_df --sender A --args object(2,0) 0
