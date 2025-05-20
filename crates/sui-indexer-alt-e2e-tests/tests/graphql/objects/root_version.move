// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# publish
module P::M {
  use sui::dynamic_field as df;
  use sui::dynamic_object_field as dof;

  public struct O has key, store {
    id: UID,
    count: u64,
  }

  public fun new(ctx: &mut TxContext): O {
    O {
      id: object::new(ctx),
      count: 0,
    }
  }

  public fun burn(outer: O) {
    let O { id, count: _ } = outer;
    id.delete();
  }

  public fun inc(outer: &mut O) {
    outer.count = outer.count + 1;
  }

  public fun add_df(parent: &mut O, name: u64, field: O) {
    df::add(&mut parent.id, name, field)
  }

  public fun remove_df(parent: &mut O, name: u64): O {
    df::remove(&mut parent.id, name)
  }

  public fun inc_df(outer: &mut O, name: u64) {
    let field: &mut O = df::borrow_mut(&mut outer.id, name);
    field.inc();
  }

  public fun add_dof(parent: &mut O, name: u64, field: O) {
    dof::add(&mut parent.id, name, field)
  }

  public fun remove_dof(parent: &mut O, name: u64): O {
    dof::remove(&mut parent.id, name)
  }

  public fun inc_dof(outer: &mut O, name: u64) {
    let field: &mut O = dof::borrow_mut(&mut outer.id, name);
    field.inc();
  }
}

//# programmable --sender A --inputs @A
//> 0: P::M::new();
//> 1: P::M::new();
//> 2: P::M::new();
//> 3: TransferObjects([Result(0), Result(1), Result(2)], Input(0))

//# programmable --sender A --inputs object(2,0) 42 object(2,1)
//> 0: P::M::add_df(Input(0), Input(1), Input(2))

//# programmable --sender A --inputs object(2,0) 43 object(2,2)
//> 0: P::M::add_dof(Input(0), Input(1), Input(2))

//# create-checkpoint

//# run-graphql
{ # When the root version lines up with an exact version, the two queries
  # return the same result.
  exact: object(address: "@{obj_2_0}", version: 4) { version }
  bounded: object(address: "@{obj_2_0}", rootVersion: 4) { version }
}

//# run-graphql
{ # Fetching the dynamic field at its parent's root version. The field will
  # exist, but the value is now wrapped and inaccessible
  field: object(address: "@{obj_3_0}", rootVersion: 4) { version }
  wrapped: object(address: "@{obj_2_1}", rootVersion: 4) { version }
}

//# run-graphql
{ # Fetching the dynamic object field at its parent's root version. Both the
  # field and the value exist because dynamic object fields don't wrap their
  # values.
  field: object(address: "@{obj_4_0}", rootVersion: 4) { version }
  wrapped: object(address: "@{obj_2_2}", rootVersion: 4) { version }
}

//# programmable --sender A --inputs object(2,0) 42
//> 0: P::M::inc_df(Input(0), Input(1))

//# programmable --sender A --inputs object(2,0) 43
//> 0: P::M::inc_dof(Input(0), Input(1))

//# create-checkpoint

//# run-graphql
{ # The dynamic field has now been updated, repeating the same query as before
  # should return the same result.
  fieldBefore: object(address: "@{obj_3_0}", rootVersion: 4) { version }
  wrappedAt4: object(address: "@{obj_2_1}", rootVersion: 4) { version }

  # Providing the version at the update, and the version after the results
  # should show the update to the dynamic field.
  fieldAfter: object(address: "@{obj_3_0}", rootVersion: 5) { version }
  wrappedAt5: object(address: "@{obj_2_1}", rootVersion: 5) { version }

  # ...and the result is unchanged at the next version, because this dynamic
  # field was not modified at this version.
  fieldLater: object(address: "@{obj_3_0}", rootVersion: 6) { version }
  wrappedAt6: object(address: "@{obj_2_1}", rootVersion: 6) { version }
}

//# run-graphql
{ # The dynamic object field has also been updated, but the previous query should
  # return the same result.
  fieldBefore: object(address: "@{obj_4_0}", rootVersion: 4) { version }
  valueBefore: object(address: "@{obj_2_2}", rootVersion: 4) { version }

  # Querying the object field at the next version should also yield the same
  # result, because the other field was modified first.
  fieldUnchanged: object(address: "@{obj_4_0}", rootVersion: 5) { version }
  valueUnchanged: object(address: "@{obj_2_2}", rootVersion: 5) { version }

  # The last version will account for the change to the dynamic object field.
  # The object representing the field will *not* change, because it doesn't
  # contain the value so has just been read, but the value's version will be
  # updated.
  fieldStillUnchanged: object(address: "@{obj_4_0}", rootVersion: 6) { version }
  valueAfter: object(address: "@{obj_2_2}", rootVersion: 6) { version }
}

//# programmable --sender A --inputs object(2,0)
//> 0: P::M::inc(Input(0))

//# create-checkpoint

//# run-graphql
{ # The parent object has been modified without touching either of the fields,
  # so we should get the same response as before when fetching them at the new
  # root version.
  fieldField: object(address: "@{obj_3_0}", rootVersion: 7) { version }
  fieldValue: object(address: "@{obj_2_1}", rootVersion: 7) { version }
  objectField: object(address: "@{obj_4_0}", rootVersion: 7) { version }
  objectValue: object(address: "@{obj_2_2}", rootVersion: 7) { version }
}

//# programmable --sender A --inputs object(2,0) 42 @A
//> 0: P::M::remove_df(Input(0), Input(1));
//> 1: TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(2,0) 43 @A
//> 0: P::M::remove_dof(Input(0), Input(1));
//> 1: TransferObjects([Result(0)], Input(2))

//# create-checkpoint

//# run-graphql
{ # The dynamic field has been deleted, so the field should not be present.
  field: object(address: "@{obj_3_0}", rootVersion: 8) { version }

  # It no longer makes sense to query the field value bounded by the parent
  # version (because it's not rooted under the parent), but nevertheless, it
  # does exist again at this version.
  unwrapped: object(address: "@{obj_2_1}", rootVersion: 8) { version }
}

//# run-graphql
{ # The dynamic object field has also been deleted, but at the previous version
  # it should still be present.
  fieldBefore: object(address: "@{obj_4_0}", rootVersion: 8) { version }
  valueBefore: object(address: "@{obj_2_2}", rootVersion: 8) { version }

  # After the deletion the field object is gone but the value is still present
  # and its version has been updated because its owner field has been modified.
  fieldAfter: object(address: "@{obj_4_0}", rootVersion: 9) { version }
  valueAfter: object(address: "@{obj_2_2}", rootVersion: 9) { version }
}

//# run-graphql
{ # Fetching an object with multiple version constraints is not supported.
  object(address: "@{obj_2_0}", version: 4, rootVersion: 4) { version }
}
