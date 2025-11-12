// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Claim a derived object, transfer an object to it, receive it with the
// derived object.

//# init --addresses a=0x0 --accounts A

//# publish
module a::m;

use sui::derived_object;

public struct Parent has key, store {
  id: UID,
}

public struct Derived has key, store {
  id: UID,
}

entry fun create_parent(ctx: &mut TxContext) {
  transfer::public_transfer(Parent { id: object::new(ctx) }, ctx.sender());
}

entry fun derive_object(parent: &mut Parent, key: u64, ctx: &mut TxContext) {
  transfer::public_transfer(
    Derived { id: derived_object::claim(&mut parent.id, key) },
    ctx.sender(),
  );
}

//# run a::m::create_parent --sender A

//# run a::m::derive_object --sender A --args object(2,0) 0

//# run a::m::derive_object --sender A --args object(2,0) 0
