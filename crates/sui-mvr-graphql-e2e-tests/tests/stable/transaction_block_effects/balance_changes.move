// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --simulator --accounts C O P Q R S

//# programmable --sender C --inputs @C 1000 2000 3000 4000 5000
//> SplitCoins(Gas, [Input(1), Input(2), Input(3), Input(4), Input(5)]);
//> TransferObjects([NestedResult(0,0), NestedResult(0,1), NestedResult(0,2), NestedResult(0,3), NestedResult(0,4)], Input(0));

//# programmable --sender C --inputs object(1,0) object(1,1) object(1,2) object(1,3) object(1,4) @O @P @Q @R @S
//> TransferObjects([Input(0)], Input(5));
//> TransferObjects([Input(1)], Input(6));
//> TransferObjects([Input(2)], Input(7));
//> TransferObjects([Input(3)], Input(8));
//> TransferObjects([Input(4)], Input(9));

//# create-checkpoint

//# advance-epoch

//# run-graphql
{
  address(address: "@{C}") {
    transactionBlocks(last: 1) {
      nodes {
        effects {
          balanceChanges {
            pageInfo {
              hasPreviousPage
              hasNextPage
              startCursor
              endCursor
            }
            edges {
              node {
                amount
              }
              cursor
            }
          }
        }
      }
    }
  }
}

//# run-graphql --cursors {"i":2,"c":1}
{
  address(address: "@{C}") {
    transactionBlocks(last: 1) {
      nodes {
        effects {
          balanceChanges(first: 2 after: "@{cursor_0}") {
            pageInfo {
              hasPreviousPage
              hasNextPage
              startCursor
              endCursor
            }
            edges {
              node {
                amount
              }
              cursor
            }
          }
        }
      }
    }
  }
}

//# run-graphql --cursors {"i":3,"c":1}
{
  address(address: "@{C}") {
    transactionBlocks(last: 1) {
      nodes {
        effects {
          balanceChanges(last: 3 before: "@{cursor_0}") {
            pageInfo {
              hasPreviousPage
              hasNextPage
              startCursor
              endCursor
            }
            edges {
              node {
                amount
              }
              cursor
            }
          }
        }
      }
    }
  }
}
