// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

// Checkpoint 1:
// A: Bag, Gas, Coin(1000), Coin(500), Coin(250), Table<u64, u64>

// Checkpoint 2:
// A: Bag, Gas, Coin(500), Coin(250), Table<u64, u64>
// B: Gas, Coin(1000)

// Checkpoint 3:
// A: Bag, Bag, Gas, Coin(500), Coin(250), Coin(200), Coin(100),
//    Table<u64, u64>, Table<address, address>

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 500 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 250 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::table::new<u64, u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) @B
//> TransferObjects([Input(0)], Input(1))

//# create-checkpoint

// Create more coins for A
//# programmable --sender A --inputs 100 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 200 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

// Create another bag for A
//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

// Create another table for A
//# programmable --sender A --inputs @A
//> 0: sui::table::new<address, address>();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-graphql
{
  all: address(address: "@{A}") {
    objects {
      pageInfo { hasNextPage }
      nodes {
        address
        version
        contents {
          type { repr }
          json
        }
      }
    }
  }

  bags: address(address: "@{A}") {
    objects(filter: { type: "0x2::bag" }) {
      pageInfo { hasNextPage }
      nodes {
        address
        version
        contents {
          type { repr }
          json
        }
      }
    }
  }

  coins: address(address: "@{A}") {
    objects(filter: { type: "0x2::coin::Coin" }) {
      pageInfo { hasNextPage }
      nodes {
        address
        version
        contents {
          type { repr }
          json
        }
      }
    }
  }

  tableU64U64: address(address: "@{A}") {
    objects(filter: { type: "0x2::table::Table<u64, u64>" }) {
      pageInfo { hasNextPage }
      nodes {
        address
        version
        contents {
          type { repr }
          json
        }
      }
    }
  }

  tableAddressAddress: address(address: "@{A}") {
    objects(filter: { type: "0x2::table::Table<address, address>" }) {
      pageInfo { hasNextPage }
      nodes {
        address
        version
        contents {
          type { repr }
          json
        }
      }
    }
  }

  doesntExist: address(address: "@{A}") {
    objects(filter: { type: "0x2::table::Table<u8, u8>" }) {
      pageInfo { hasNextPage }
      nodes {
        address
        version
        contents {
          type { repr }
          json
        }
      }
    }
  }
}

//# run-graphql
{
  timeTravel: checkpoint(sequenceNumber: 2) {
    query {
      address(address: "@{A}") {
        objects {
          pageInfo { hasNextPage }
          nodes {
            address
            version
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
