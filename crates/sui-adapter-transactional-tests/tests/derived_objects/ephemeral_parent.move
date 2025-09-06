// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Create a derived object out of a non-fresh UID.

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

entry fun ephemeral_parent_persistent_derived(ctx: &mut TxContext) {
  let mut parent_uid = object::new(ctx);
  let derived = Derived { id: derived_object::claim(&mut parent_uid, 0) };
  parent_uid.delete();
  transfer::public_transfer(derived, ctx.sender());
}

entry fun ephemeral_parent_ephemeral_derived(ctx: &mut TxContext) {
  let mut parent_uid = object::new(ctx);
  let derived_uid = derived_object::claim(&mut parent_uid, 0);
  parent_uid.delete();
  derived_uid.delete();
}

entry fun ephemeral_parent_ephemeral_intermediate_derived(ctx: &mut TxContext) {
  let mut parent_uid = object::new(ctx);
  let mut derived_uid = derived_object::claim(&mut parent_uid, 0);
  let nested_derived_uid = derived_object::claim(&mut derived_uid, 0);
  parent_uid.delete();
  derived_uid.delete();
  transfer::public_transfer(Derived { id: nested_derived_uid }, ctx.sender());
}

//# run a::m::ephemeral_parent_persistent_derived --sender A

//# view-object 2,0

//# view-object 2,1

//# run a::m::ephemeral_parent_ephemeral_derived --sender A

//# view-object 5,0

//# run a::m::ephemeral_parent_ephemeral_intermediate_derived --sender A

//# view-object 7,0

//# view-object 7,1

//# view-object 7,2
