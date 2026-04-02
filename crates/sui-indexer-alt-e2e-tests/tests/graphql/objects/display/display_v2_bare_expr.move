// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A --addresses test=0x0 --simulator --protocol-version 118

//# publish --sender A
module test::mod {
  use std::string::String;
  use sui::display_registry::DisplayCap;
  use sui::display_registry::DisplayRegistry;

  public enum Status has copy, drop, store {
    Pending { message: String },
  }

  public struct Inner has copy, drop, store {
    count: u64,
    label: String,
  }

  public struct Foo has key, store {
    id: UID,
    label: String,
    inner: Inner,
    status: Status,
    nums: vector<u64>,
  }

  public fun display(registry: &mut DisplayRegistry, ctx: &mut TxContext): DisplayCap<Foo> {
    let (mut display, cap) = registry.new(internal::permit(), ctx);
    display.set(&cap, "label_implicit", "{label}");
    display.set(&cap, "inner_implicit", "{inner}");
    display.set(&cap, "status_implicit", "{status}");
    display.set(&cap, "nums_implicit", "{nums}");
    display.set(&cap, "inner_json", "{inner:json}");
    display.share();
    cap
  }

  public fun new(ctx: &mut TxContext): Foo {
    Foo {
      id: object::new(ctx),
      label: b"hello".to_string(),
      inner: Inner {
        count: 42,
        label: b"inside".to_string(),
      },
      status: Status::Pending { message: b"ready".to_string() },
      nums: vector[1, 2, 3],
    }
  }
}

//# programmable --sender A --inputs object(0xd) @A
//> 0: test::mod::display(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @A
//> 0: test::mod::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-graphql
{
  object(address: "@{obj_3_0}") {
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
