// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses T=0x0 --accounts A B --simulator

//# publish --sender A
module T::test {
    use sui::coin;

    public struct TEST has drop {}

    fun init(otw: TEST, ctx: &mut TxContext){
        let (mut treasury, metadata) =
            coin::create_currency(otw, 6, b"", b"", b"", option::none(), ctx);

        // Mint and transfer TEST coins with different amounts
        transfer::public_transfer(treasury.mint(1_000000, ctx), ctx.sender());
        transfer::public_transfer(treasury.mint(0_500000, ctx), ctx.sender());
        transfer::public_transfer(treasury.mint(0_250000, ctx), ctx.sender());
        transfer::public_transfer(treasury.mint(0_100000, ctx), ctx.sender());

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, ctx.sender());
    }
}

//# create-checkpoint

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 500 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) @B
//> TransferObjects([Input(0)], Input(1))

//# programmable --sender A --inputs object(3,0) @B
//> TransferObjects([Input(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Account A queries
  accountA: address(address: "@{A}") {
    # All objects including coins and bags
    allObjects: objects {
      pageInfo { hasNextPage }
      nodes {
        address
        contents {
          type { repr }
          json
        }
      }
    }

    # All coins (no marker filter)
    allCoins: objects(filter: { type: "0x2::coin::Coin" }) {
      pageInfo { hasNextPage }
      nodes {
        address
        contents {
          type { repr }
          json
        }
      }
    }


    # Only TEST coin objects
    testCoins: objects(filter: { type: "0x2::coin::Coin<@{T}::test::TEST>" }) {
      pageInfo { hasNextPage }
      nodes {
        address
        contents {
          json
        }
      }
    }

    # Only SUI coin objects
    suiCoins: objects(filter: { type: "0x2::coin::Coin<0x2::sui::SUI>" }) {
      pageInfo { hasNextPage }
      nodes {
        address
        contents {
          json
        }
      }
    }
  }

  # Account B queries
  accountB: address(address: "@{B}") {
    # All objects for B
    allObjects: objects {
      pageInfo { hasNextPage }
      nodes {
        address
        contents {
          type { repr }
          json
        }
      }
    }

    # Only TEST coin objects for B
    testCoins: objects(filter: { type: "0x2::coin::Coin<@{T}::test::TEST>" }) {
      pageInfo { hasNextPage }
      nodes {
        address
        contents {
          json
        }
      }
    }

    # Only SUI coin objects for B
    suiCoins: objects(filter: { type: "0x2::coin::Coin<0x2::sui::SUI>" }) {
      pageInfo { hasNextPage }
      nodes {
        address
        contents {
          json
        }
      }
    }
  }
}

//# run-graphql
{ # Time travel to checkpoint 2 (before transfers)
  timeTravel: checkpoint(sequenceNumber: 2) {
    query {
      address(address: "@{A}") {
        # Should have all coin objects at this point
        objects(filter: { type: "0x2::coin::Coin" }) {
          pageInfo { hasNextPage }
          nodes {
            address
            contents {
              type { repr }
              json
            }
          }
        }
      }
    }
  }
}
