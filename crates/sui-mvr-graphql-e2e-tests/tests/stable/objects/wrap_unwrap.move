// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 --accounts A --simulator

//# publish
module P0::m {
    public struct Foo has key, store {
        id: UID,
    }

    public struct Bar has key, store {
        id: UID,
        foo: Foo,
    }

    public fun foo(ctx: &mut TxContext): Foo {
        Foo { id: object::new(ctx) }
    }

    public fun from_foo(foo: Foo, ctx: &mut TxContext): Bar {
        Bar { id: object::new(ctx), foo }
    }

    public fun into_foo(bar: Bar): Foo {
        let Bar { id, foo } = bar;
        object::delete(id);
        foo
    }
}

//# programmable --sender A --inputs @A
//> 0: P0::m::foo();
//> TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A object(2,0)
//> 0: P0::m::from_foo(Input(1));
//> TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A object(3,0)
//> 0: P0::m::into_foo(Input(1));
//> TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-graphql
{
  object1: object(address: "@{obj_2_0}") {
    digest
  }
  object2: object(address: "@{obj_3_0}") {
    digest
  }
}

//# programmable --sender A --inputs @A
//> 0: P0::m::foo();
//> TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A object(7,0)
//> 0: P0::m::from_foo(Input(1));
//> TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-graphql
{
  object1: object(address: "@{obj_7_0}") {
    digest
  }
  object2: object(address: "@{obj_8_0}") {
    digest
  }
}

//# programmable --sender A --inputs @A object(8,0)
//> 0: P0::m::into_foo(Input(1));
//> TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-graphql
{
  object1: object(address: "@{obj_7_0}") {
    digest
  }
  object2: object(address: "@{obj_8_0}") {
    digest
  }
}
