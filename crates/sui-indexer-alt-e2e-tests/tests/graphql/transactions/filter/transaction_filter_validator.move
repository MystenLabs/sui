// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses Test=0x0 --accounts A --simulator

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

//# create-checkpoint

//# run-graphql
{
  transactions(filter: { function: "@{Test}::M1::create", kind: PROGRAMMABLE_TX }) {
    edges {
      cursor
    }
  }
}

//# run-graphql
{
  transactions(filter: { function: "@{Test}::M1::create", affectedAddress: "@{A}" }) {
    edges {
      cursor
    }
  }
}

//# run-graphql
{
  transactions(filter: { kind: PROGRAMMABLE_TX, affectedAddress: "@{A}" }) {
    edges {
      cursor
    }
  }
}
