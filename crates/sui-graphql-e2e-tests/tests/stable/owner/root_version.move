// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 --accounts A --simulator

//# publish
module P0::M {
    use sui::dynamic_object_field as dof;

    public struct O has key {
        id: UID,
        count: u64,
        wrapped: Option<W>,
    }

    public struct W has key, store {
        id: UID,
        count: u64,
    }

    public struct DOF has key, store {
        id: UID,
        count: u64,
    }

    // Constructors for each part of the chain

    entry fun new_o(ctx: &mut TxContext) {
        transfer::transfer(
            O {
                id: object::new(ctx),
                wrapped: option::none(),
                count: 0,
            },
            ctx.sender(),
        );
    }

    entry fun new_w(ctx: &mut TxContext) {
        transfer::transfer(
            W { id: object::new(ctx), count: 0 },
            ctx.sender(),
        );
    }

    entry fun new_dof(ctx: &mut TxContext) {
        transfer::transfer(
            DOF { id: object::new(ctx), count: 0 },
            ctx.sender(),
        );
    }

    entry fun connect(o: &mut O, mut w: W, mut inner: DOF, outer: DOF) {
        dof::add(&mut inner.id, false, outer);
        dof::add(&mut w.id, false, inner);
        o.wrapped.fill(w);
    }

    /// Touch just the outer object (nothing else changes).
    entry fun touch_root(o: &mut O) {
        o.count = o.count + 1;
    }

    /// Touch the wrapped object.
    entry fun touch_wrapped(o: &mut O) {
        let w = o.wrapped.borrow_mut();
        w.count = w.count + 1;
    }

    /// Touch the inner dynamic object field.
    entry fun touch_inner(o: &mut O) {
        let w = o.wrapped.borrow_mut();
        let inner: &mut DOF = dof::borrow_mut(&mut w.id, false);
        inner.count = inner.count + 1;
    }

    /// Touch the inner dynamic object field.
    entry fun touch_outer(o: &mut O) {
        let w = o.wrapped.borrow_mut();
        let inner: &mut DOF = dof::borrow_mut(&mut w.id, false);
        let outer: &mut DOF = dof::borrow_mut(&mut inner.id, false);
        outer.count = outer.count + 1;
    }
}

// Create all the objects

//# run P0::M::new_o
// lamport version: 3 (o)

//# run P0::M::new_w
// lamport version: 4 (w)

//# run P0::M::new_dof
// lamport version: 5 (inner)

//# run P0::M::new_dof
// lamport version: 6 (outer)

//# run P0::M::connect --args object(2,0) object(3,0) object(4,0) object(5,0)
// lamport version: 7 (o, w, inner, outer)
// Create a chain from the created objects:
//    o -(wraps)-> w -(dof)-> inner -(dof)-> outer

//# view-object 2,0

// Nudge each level of the chain in turn:

//# run P0::M::touch_root --args object(2,0)
// lamport version: 8 (o)

//# run P0::M::touch_wrapped --args object(2,0)
// lamport version: 9 (o)

//# run P0::M::touch_inner --args object(2,0)
// lamport version: 10 (o, inner)

//# run P0::M::touch_outer --args object(2,0)
// lamport version: 11 (o, outer)

//# view-object 2,0

//# create-checkpoint

//# run-graphql
fragment Obj on Owner {
  asObject {
    asMoveObject {
      version
      contents { json }
    }
  }
}

{ # Queries for the root object
  latest: owner(address: "@{obj_2_0}") { ...Obj }
  versioned: owner(address: "@{obj_2_0}", rootVersion: 10) { ...Obj }
  beforeWrappedBump: owner(address: "@{obj_2_0}", rootVersion: 8) { ...Obj }
  beforeBump: owner(address: "@{obj_2_0}", rootVersion: 7) { ...Obj }
}

//# run-graphql
fragment DOF on Owner {
  dynamicObjectField(name: { type: "bool", bcs: "AA==" }) {
    value {
      ... on MoveObject {
        version
        contents { json }
      }
    }
  }
}

{ # Querying dynamic fields under the wrapped Move object
  # AA== is the base64 encoding of the boolean value `false` (0x00).

  # Accessing an ID as an Owner imposes no version constraint, so we will end
  # up fetching the latest versions of the dynamic object fields.
  unversioned: owner(address: "@{obj_3_0}") { ...DOF }

  # Specifying the latest version of the wrapping object has the
  # desired effect.
  latest: owner(address: "@{obj_3_0}", rootVersion: 11) { ...DOF }

  # Look at various versions of the object in history
  afterFirstInnerBump: owner(address: "@{obj_3_0}", rootVersion: 10) { ...DOF }
  beforeFirstInnerBump: owner(address: "@{obj_3_0}", rootVersion: 9) { ...DOF }
  beforeBumps: owner(address: "@{obj_3_0}", rootVersion: 7) { ...DOF }
}

//# run-graphql
fragment DOF on Owner {
  dynamicObjectField(name: { type: "bool", bcs: "AA==" }) {
    value {
      ... on MoveObject {
        version
        contents { json }
      }
    }
  }
}

{ # Querying a nested dynamic field, where the version of the child
  # may be greater than the version of its immediate parent

  # Accessing the outer ID as an owner imposes no version constraint, so we see
  # the latest version of the inner object.
  unversioned: owner(address: "@{obj_4_0}") { ...DOF }

  # At its latest version as an object, it doesn't see the latest change on its
  # child.
  latestObject: object(address: "@{obj_4_0}") {
    dynamicObjectField(name: { type: "bool", bcs: "AA==" }) {
      value {
        ... on MoveObject {
          version
          contents { json }
        }
      }
    }
  }

  # But at its root's latest version, it does
  latest: owner(address: "@{obj_4_0}", rootVersion: 11) { ...DOF }
  beforeInnerBump: owner(address: "@{obj_4_0}", rootVersion: 10) { ...DOF }
}
