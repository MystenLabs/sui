// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

//# publish
module test::simple {
    public struct Counter has key {
        id: UID,
        value: u64,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Counter {
            id: object::new(ctx),
            value: 0,
        })
    }

    public fun increment(counter: &mut Counter) {
        counter.value = counter.value + 1;
    }
}

//# programmable --sender A --inputs object(1,0)
//> 0: test::simple::increment(Input(0));

//# create-checkpoint

//# run-graphql
{
  transaction(digest: "@{digest_2}") {
    gasInput {
      gasSponsor {
        address
      }
      gasPrice
      gasBudget
      gasPayment {
        nodes {
          address
          version
        }
      }
    }
  }
}
