// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 A=0x42 --simulator

//# publish
module Test::M1 {
    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }
}

//# run Test::M1::create --args 0 @A

//# run Test::M1::create --args 1 @A

//# run Test::M1::create --args 2 @A

//# run Test::M1::create --args 3 @A

//# run Test::M1::create --args 4 @A

//# create-checkpoint

//# run-graphql
{
  # select all objects owned by A
  address(address: "@{A}") {
    objects {
      edges {
        cursor
      }
    }
  }
}

//# run-graphql
{
  # select the first 2 objects owned by A
  address(address: "@{A}") {
    objects(first: 2) {
      edges {
        cursor
      }
    }
  }
}

//# run-graphql --cursors bcs(@{obj_5_0},@{highest_checkpoint})
{
  address(address: "@{A}") {
    # select the 2nd and 3rd objects
    # note that order does not correspond
    # to order in which objects were created
    objects(first: 2 after: "@{cursor_0}") {
      edges {
        cursor
      }
    }
  }
}

//# run-graphql --cursors bcs(@{obj_4_0},@{highest_checkpoint})
{
  address(address: "@{A}") {
    # select 4th and last object
    objects(first: 2 after: "@{cursor_0}") {
      edges {
        cursor
      }
    }
  }
}

//# run-graphql --cursors bcs(@{obj_3_0},@{highest_checkpoint})
{
  address(address: "@{A}") {
    # select 3rd and 4th object
    objects(last: 2 before: "@{cursor_0}") {
      edges {
        cursor
      }
    }
  }
}

//# run-graphql
{
  address(address: "@{A}") {
    objects(last: 2) {
      edges {
        cursor
      }
    }
  }
}
