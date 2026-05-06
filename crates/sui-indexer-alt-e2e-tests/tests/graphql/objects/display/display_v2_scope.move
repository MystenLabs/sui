// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A --addresses test=0x0 --simulator --protocol-version 118

//# publish --sender A
module test::mod {
  use std::string::String;
  use sui::display_registry::DisplayCap;
  use sui::display_registry::DisplayRegistry;

  public struct Foo has key, store {
    id: UID,
    bar: Bar,
  }

  public struct Bar has store {
    label: String,
    count: u64,
  }

  public fun new(label: String, count: u64, ctx: &mut TxContext): Foo {
    Foo {
      id: object::new(ctx),
      bar: Bar { label, count },
    }
  }

  public fun display(registry: &mut DisplayRegistry, ctx: &mut TxContext): DisplayCap<Foo> {
    let (mut display, cap) = registry.new(internal::permit(), ctx);
    display.set(&cap, "label", "{bar.label}");
    display.set(&cap, "bar", "{bar:json}");
    display.share();
    cap
  }
}

//# programmable --sender A --inputs @A "hello" 42u64
//> 0: test::mod::new(Input(1), Input(2));
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(0xd) @A
//> 0: test::mod::display(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  object(address: "@{obj_2_0}") {
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
