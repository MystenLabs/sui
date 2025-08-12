// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests that shared objects can have dynamic fields added
// dynamic fields can be added and removed in the same transaction

//# init --addresses a=0x0 --accounts A --shared-object-deletion true --enable-non-exclusive-write-objects

//# publish
module a::m;

use sui::dynamic_field::{add, remove};

public struct Obj has key, store {
  id: object::UID,
  val: u64,
}

public fun create(ctx: &mut TxContext) {
  transfer::public_share_object(Obj { id: object::new(ctx), val: 0 })
}

public fun add_dynamic_field(obj: &mut Obj) {
  add<u64, u64>(&mut obj.id, 0, 42);
}

public fun mutate(obj: &mut Obj) {
  obj.val = 1;
}

public fun by_value_with_mutation(mut obj: Obj) {
  obj.val = 2;
  transfer::public_share_object(obj);
}

public fun by_value_delete(obj: Obj) {
  let Obj { id, val: _ } = obj;
  object::delete(id);
}

public fun by_value_without_mutation(obj: Obj) {
  transfer::public_share_object(obj);
}

public struct Wrapper {
  obj: Obj,
}

public fun by_value_without_mutation_wrapped(obj: Obj) {
  let wrapper = Wrapper { obj };
  let Wrapper { obj } = wrapper;
  transfer::public_share_object(obj);
}

public fun remove_and_transfer(obj: &mut Obj, ctx: &mut TxContext) {
  let val = remove<u64, u64>(&mut obj.id, 0);
  transfer::public_share_object(Obj { id: object::new(ctx), val });
}

//# run a::m::create --sender A

//# view-object 2,0

// Can add dynamic field with non-exclusive write
//# run a::m::add_dynamic_field --sender A --args nonexclusive(2,0)

// version is not bumped
//# view-object 2,0

// Will fail, new dynamic field should not yet be visible
//# run a::m::remove_and_transfer --sender A --args object(2,0)

// Version has now been bumped by previous transaction, so field is visible.
// (normally this would be done by a barrier transaction instead of another user transaction)
//# run a::m::remove_and_transfer --sender A --args object(2,0)

//# view-object 2,0

//# view-object 6,0

// Attempt to mutate shared object with non-exclusive write causes abort
//# run a::m::mutate --sender A --args nonexclusive(2,0)

//# view-object 2,0

// Can mutate mutable shared object
//# run a::m::mutate --sender A --args object(2,0)

//# view-object 2,0

// Passing by value without mutation is allowed
//# run a::m::by_value_without_mutation --sender A --args nonexclusive(2,0)

// Passing by value without mutation is allowed with wrap/unwrap
//# run a::m::by_value_without_mutation_wrapped --sender A --args nonexclusive(2,0)

// Passing by value with mutation is not allowed
//# run a::m::by_value_with_mutation --sender A --args nonexclusive(2,0)

// Deleting is not allowed
//# run a::m::by_value_delete --sender A --args nonexclusive(2,0)

//# view-object 2,0
