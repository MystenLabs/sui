// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO: set protocol version once it's no longer the max
//# init --accounts A --addresses test=0x0 --simulator

//# publish --sender A
module test::mod {
  use std::string::String;
  use sui::display_registry::DisplayRegistry;
  use sui::display_registry::DisplayCap;
  use sui::dynamic_field as df;

  public struct Foo has key, store {
    id: UID,
    bar: Bar,
  }

  public struct Bar has store { baz: Baz, val: u64 }
  public struct Baz has store { qux: Qux, val: bool }
  public struct Qux has store { quy: Quy, val: String }
  public struct Quy has store { quz: Quz, val: Option<ID> }
  public struct Quz has store { val: u8 }

  /// Register a Display v2 grammar for `Foo`.
  public fun display(registry: &mut DisplayRegistry, ctx: &mut TxContext): DisplayCap<Foo> {
    let (mut display, cap) = registry.new(internal::permit(), ctx);

    display.set(&cap, "bar", "bar is {bar.val}!");
    display.set(&cap, "baz", "baz is {bar.baz.val}?");
    display.set(&cap, "qux", "{bar.baz.qux:json}");
    display.set(&cap, "quy", "quy is {bar.baz.qux.quy.val}.");
    display.set(&cap, "qu_", "x({bar.baz.qux.val}) y({bar.baz.qux.quy.val}), z({bar.baz.qux.quy.quz.val})?!");
    display.set(&cap, "f42", "[42] is {id->[42u64] | 0x00420042u32 :hex}");
    display.share();

    cap
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

  public fun add_df(foo: &mut Foo) {
    df::add(&mut foo.id, 42u64, 0x42004200u32);
  }
}

//# programmable --sender A --inputs object(0xd) @A
//> 0: test::mod::display(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @A 42 true "hello" 43u8
//> 0: std::option::some<sui::object::ID>(Input(0));
//> 1: test::mod::new(Input(1), Input(2), Input(3), Result(0), Input(4));
//> 2: TransferObjects([Result(1)], Input(0))

//# programmable --sender A --inputs @A 42 true "hello" 43u8
//> 0: std::option::none<sui::object::ID>();
//> 1: test::mod::new(Input(1), Input(2), Input(3), Result(0), Input(4));
//> 2: TransferObjects([Result(1)], Input(0))

//# programmable --sender A --inputs object(4,0)
//> 0: test::mod::add_df(Input(0))

//# create-checkpoint

//# run-graphql
{
  multiGetObjects(keys: [
    { address: "@{obj_3_0}" },
    { address: "@{obj_4_0}" }
  ]) {
    asMoveObject {
      contents {
        display {
          output
          errors
        }
      }
    }
  }
}
