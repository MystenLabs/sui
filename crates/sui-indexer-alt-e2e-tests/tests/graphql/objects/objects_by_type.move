// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# programmable --sender A --inputs 10 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs 20 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @B
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs 30 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# programmable --sender A --inputs 40 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @B
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs 50 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs 60 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs @B
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-graphql
{ # Display all transactions so we can identify which objects belong where.
  transactions(first: 20) {
    pageInfo { hasNextPage }
    nodes {
      effects {
        objectChanges {
          pageInfo { hasNextPage }
          nodes {
            address
            idCreated
            outputState { version }
          }
        }
      }
    }
  }
}

//# run-graphql
{ # Filter by coin module
  objects(filter: {type: "0x2::coin"}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }

    nodes {
      address
      version
    }
  }
}

//# run-graphql
{ # Filter by bag module
  objects(filter: {type: "0x2::bag"}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }

    nodes {
      address
      version
    }
  }
}

//# run-graphql
{ # Filter by full coin type
  objects(filter: {type: "0x2::coin::Coin"}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }

    nodes {
      address
      version
    }
  }
}

//# run-graphql
{ # Filter by full bag type
  objects(filter: {type: "0x2::bag::Bag"}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }

    nodes {
      address
      version
    }
  }
}

//# run-graphql
{ # Filter by instantiated coin type
  objects(filter: {type: "0x2::coin::Coin<0x2::sui::SUI>"}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }

    nodes {
      address
      version
    }
  }
}

//# run-graphql
{ # Test pagination with first limit
  objects(filter: {type: "0x2::coin::Coin"}, first: 2) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }

    nodes {
      address
      version
    }
  }
}

//# run-graphql
{ # Test pagination with last limit
  objects(filter: {type: "0x2::bag::Bag"}, last: 2) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }

    nodes {
      address
      version
    }
  }
}

//# run-graphql
{ # Time travel: Query coins at checkpoint 1 (should have 3 coins + genesis gas)
  checkpoint(sequenceNumber: 1) {
    query {
      objects(filter: {type: "0x2::coin::Coin"}) {
        pageInfo {
          hasPreviousPage
          hasNextPage
        }

        nodes {
          address
          version
        }
      }
    }
  }
}

//# run-graphql
{ # Time travel: Query bags at checkpoint 1 (should have 3 bags)
  checkpoint(sequenceNumber: 1) {
    query {
      objects(filter: {type: "0x2::bag::Bag"}) {
        pageInfo {
          hasPreviousPage
          hasNextPage
        }

        nodes {
          address
          version
        }
      }
    }
  }
}

//# run-graphql
{ # Time travel: Test pagination at checkpoint 1 with first limit
  checkpoint(sequenceNumber: 1) {
    query {
      objects(filter: {type: "0x2::coin::Coin"}, first: 2) {
        pageInfo {
          hasPreviousPage
          hasNextPage
        }

        nodes {
          address
          version
        }
      }
    }
  }
}

//# run-graphql
{ # Time travel: Test pagination at checkpoint 1 with last limit
  checkpoint(sequenceNumber: 1) {
    query {
      objects(filter: {type: "0x2::bag::Bag"}, last: 2) {
        pageInfo {
          hasPreviousPage
          hasNextPage
        }

        nodes {
          address
          version
        }
      }
    }
  }
}
