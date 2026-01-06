// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Try different kinds of ownership for a freshly derived object.

//# init --addresses a=0x0 --accounts A

//# publish
module a::m;

use sui::derived_object;
use sui::party;

public struct Parent has key, store {
  id: UID,
}

public struct Derived has key, store {
  id: UID,
}

entry fun create_parent(ctx: &mut TxContext) {
  transfer::public_transfer(Parent { id: object::new(ctx) }, ctx.sender());
}

entry fun derive_and_share(parent: &mut Parent) {
  let derived = Derived { id: derived_object::claim(&mut parent.id, 0u64) };
  transfer::share_object(derived);
}

entry fun derive_and_make_party(parent: &mut Parent, ctx: &TxContext) {
  let derived = Derived { id: derived_object::claim(&mut parent.id, 1u64) };
  party::single_owner(ctx.sender()).transfer!(derived);
}

entry fun derive_and_make_immutable(parent: &mut Parent) {
  let derived = Derived { id: derived_object::claim(&mut parent.id, 2u64) };
  transfer::freeze_object(derived);
}

//# run a::m::create_parent --sender A

//# run a::m::derive_and_share --sender A --args object(2,0)

//# run a::m::derive_and_make_party --sender A --args object(2,0)

//# view-object 4,0

// transfer this object back to single owner
//# transfer-object 4,0 --sender A --recipient A

//# view-object 4,0

//# run a::m::derive_and_make_immutable --sender A --args object(2,0)

//# view-object 8,0
