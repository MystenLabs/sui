// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B C --addresses test=0x0 --simulator

// Transaction with balance changes - transfer SUI from A to B
//# programmable --sender A --inputs 1000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

// Transaction with multiple balance changes - transfer to multiple recipients
//# programmable --sender A --inputs 500 @B 300 @C
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: SplitCoins(Gas, [Input(2)]);
//> 2: TransferObjects([Result(0)], Input(1));
//> 3: TransferObjects([Result(1)], Input(3))

//# create-checkpoint

//# run-graphql
{ # Test balance_changes field on single transfer transaction
  singleTransferTransaction: transactionEffects(digest: "@{digest_1}") {
    balanceChanges {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      nodes {
        owner {
          address
        }
        coinType
        amount
      }
    }
  }
}

//# run-graphql
{ # Test balance_changes field on multiple transfer transaction
  multipleTransferTransaction: transactionEffects(digest: "@{digest_2}") {
    balanceChanges {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      nodes {
        owner {
            address
        }
        coinType
        amount
      }
    }
  }
}

//# run-graphql
{ # Test balance_changes field with pagination
  paginatedBalanceChanges: transactionEffects(digest: "@{digest_2}") {
    balanceChanges(first: 2) {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      edges {
        cursor
        node {
          owner {
            address
          }
          coinType
          amount
        }
      }
    }
  }
}
