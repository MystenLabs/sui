// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test the change of APY with heavy transactions

//# init --protocol-version 51 --simulator --accounts A --addresses P0=0x0

//# advance-epoch

//# create-checkpoint

//# publish --sender A --gas-budget 9999999999
module P0::m {
    public struct Big has key, store {
        id: UID,
        weight: vector<u8>,
    }

    fun weight(): vector<u8> {
        let mut i = 0;
        let mut v = vector[];
        while (i < 248 * 1024) {
            vector::push_back(&mut v, 42);
            i = i + 1;
        };
        v
    }

    public entry fun new(ctx: &mut TxContext){
        let id = object::new(ctx);
        let w = weight();
        sui::transfer::public_transfer(
            Big { id, weight: w },
            ctx.sender()
        )
    }
}

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# create-checkpoint

//# advance-epoch

//# run-graphql
{
  epoch(id: 1) {
    validatorSet {
      activeValidators {
        nodes {
          apy
          name
        }
      }
    }
  }
}

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# run P0::m::new --sender A

//# create-checkpoint

//# advance-epoch

// check the epoch metrics

//# run-graphql
{
  epoch(id: 2) {
    validatorSet {
      activeValidators {
        nodes {
          apy
          name
          reportRecords {
            nodes {
              address
            }
          }
        }
      }
    }
  }
}
