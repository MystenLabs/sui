// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

// 1. Publish a package that includes a Display format.
// 2. Create some objects from this package.
// 3. View the Display format of those objects.
// 4. Edit the Display format of the object.
// 5. View the updated Display format of the object.

//# publish --sender A
module test::mod {
  use std::string::{String, utf8};
  use sui::display;
  use sui::package;

  public struct MOD() has drop;

  public struct Foo has key, store {
    id: UID,
    bar: Bar,
  }

  public struct Bar has store { baz: Baz, val: u64 }
  public struct Baz has store { qux: Qux, val: bool }
  public struct Qux has store { quy: Quy, val: String }
  public struct Quy has store { quz: Quz, val: Option<ID> }
  public struct Quz has store { val: u8 }

  fun init(otw: MOD, ctx: &mut TxContext) {
    let publisher = package::claim(otw, ctx);
    let mut d = display::new_with_fields<Foo>(
      &publisher,
      vector[
        utf8(b"bar"),
        utf8(b"baz"),
        utf8(b"quy"),
        utf8(b"qu_"),
      ],
      vector[
        utf8(b"bar is {bar.val}!"),
        utf8(b"baz is {bar.baz.val}?"),
        utf8(b"quy is {bar.baz.qux.quy}."),
        utf8(b"x({bar.baz.qux.val}) y({bar.baz.qux.quy.val}), z({bar.baz.qux.quy.quz.val})?!"),
      ],
      ctx,
    );

    d.update_version();
    transfer::public_transfer(publisher, ctx.sender());
    transfer::public_transfer(d, ctx.sender());
  }

  public fun new(
    v_bar: u64,
    v_baz: bool,
    v_qux: String,
    v_quy: Option<ID>,
    v_quz: u8,
    ctx: &mut TxContext,
  ): Foo {
    let quz = Quz { val: v_quz };
    let quy = Quy { val: v_quy, quz };
    let qux = Qux { val: v_qux, quy };
    let baz = Baz { val: v_baz, qux };
    let bar = Bar { val: v_bar, baz };
    Foo { id: object::new(ctx), bar }
  }
}

//# programmable --sender A --inputs @A 42 true "hello" 43u8
//> 0: std::option::some<sui::object::ID>(Input(0));
//> 1: test::mod::new(Input(1), Input(2), Input(3), Result(0), Input(4));
//> 2: TransferObjects([Result(1)], Input(0))

//# programmable --sender A --inputs @A 42 true "hello" 43u8
//> 0: std::option::none<sui::object::ID>();
//> 1: test::mod::new(Input(1), Input(2), Input(3), Result(0), Input(4));
//> 2: TransferObjects([Result(1)], Input(0))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_1_0}", { "showType": true }]
}

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_1_1}", { "showType": true }]
}

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_1_2}", { "showType": true }]
}

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_2_0}", { "showDisplay": true }]
}

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_3_0}", { "showDisplay": true }]
}

//# programmable --sender A --inputs object(1,1) "quy" "{bar.baz.qux.quy.val}!"
//> 0: sui::display::edit<test::mod::Foo>(Input(0), Input(1), Input(2));
//> 1: sui::display::update_version<test::mod::Foo>(Input(0));

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_2_0}", { "showDisplay": true }]
}

//# run-jsonrpc
{
  "method": "sui_getObject",
  "params": ["@{obj_3_0}", { "showDisplay": true }]
}
