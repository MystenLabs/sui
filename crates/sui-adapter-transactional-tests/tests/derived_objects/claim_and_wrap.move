// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Claim a derived UID and wrap it in its parent.

//# init --addresses a=0x0 --accounts A

//# publish
module a::m;

use sui::derived_object;

public struct Parent has key, store {
  id: UID,
  wrapped: Option<UID>,
}

entry fun create_parent(ctx: &mut TxContext) {
  transfer::public_transfer(Parent { id: object::new(ctx), wrapped: option::none() }, ctx.sender());
}

entry fun claim_and_wrap(parent: &mut Parent, key: u64) {
  parent.wrapped.fill(derived_object::claim(&mut parent.id, key));
}

//# run a::m::create_parent --sender A

//# run a::m::claim_and_wrap --sender A --args object(2,0) 0
