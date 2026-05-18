// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 118 --accounts A --addresses test=0x0 --simulator

//# publish --sender A
module test::mod {
  use sui::derived_object;
  use sui::dynamic_field as df;

  public struct Root has key, store {
    id: UID,
    key: u64,
  }

  public struct Parent has key, store {
    id: UID,
  }

  public struct Child has key, store {
    id: UID,
    count: u64,
  }

  public fun root(key: u64, ctx: &mut TxContext): Root {
    Root {
      id: object::new(ctx),
      key,
    }
  }

  public fun parent(ctx: &mut TxContext): Parent {
    Parent { id: object::new(ctx) }
  }

  public fun dynamic_field(parent: &mut Parent, key: u64, count: u64, ctx: &mut TxContext) {
    let value = Child {
      id: object::new(ctx),
      count,
    };

    df::add(&mut parent.id, key, value);
  }

  public fun derived(parent: &mut Parent, key: u64, count: u64): Child {
    Child {
      id: derived_object::claim(&mut parent.id, key),
      count,
    }
  }

  public fun inc_field(parent: &mut Parent, key: u64) {
    let child: &mut Child = df::borrow_mut(&mut parent.id, key);
    child.inc_child();
  }

  public fun inc_child(child: &mut Child) {
    child.count = child.count + 1;
  }
}

//# programmable --sender A --inputs @A 7u64
//> 0: test::mod::root(Input(1));
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: test::mod::parent();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# programmable --sender A --inputs object(3,0) 7u64 111u64
//> 0: test::mod::dynamic_field(Input(0), Input(1), Input(2))

//# programmable --sender A --inputs object(3,0) 7u64
//> 0: test::mod::inc_field(Input(0), Input(1))

//# programmable --sender A --inputs object(3,0) 7u64 333u64 @A
//> 0: test::mod::derived(Input(0), Input(1), Input(2));
//> 1: TransferObjects([Result(0)], Input(3))

//# programmable --sender A --inputs object(7,1)
//> 0: test::mod::inc_child(Input(0))

//# create-checkpoint

//# run-graphql
{
  object(address: "@{obj_2_0}", rootVersion: 3) {
    asMoveObject {
      contents {
        dynamicField: format(format: "{@@{obj_3_0}->[key].count}")
        derivedObject: format(format: "{@@{obj_3_0}~>[key].count}")
      }
    }
  }
}
