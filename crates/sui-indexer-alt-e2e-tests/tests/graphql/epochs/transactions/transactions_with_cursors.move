// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# advance-epoch

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 43 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# programmable --sender A --inputs 2
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# create-checkpoint

//# programmable --sender A --inputs 3
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs 3
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs 3
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# advance-epoch

//# run-graphql --cursors 4 8
{ # Fetch an epoch and its transactions, with cursors applied to transactions
  epoch1WithCursorsFromFront: epoch(epochId: 1) {
    totalTransactions
    transactions(first: 2, after: "@{cursor_0}", before: "@{cursor_1}") {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql --cursors 4 8
{ # Fetch an epoch and its transactions, with cursors applied to transactions
  epoch1WithCursorsFromBack: epoch(epochId: 1) {
    totalTransactions
    transactions(last: 2, after: "@{cursor_0}", before: "@{cursor_1}") {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql --cursors 4 
{ # Fetch an epoch and its transactions, paginate from the front
  epoch1WithAfterCursorFromFront: epoch(epochId: 1) {
    totalTransactions
    transactions(first: 2, after: "@{cursor_0}") {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql --cursors 4 
{ # Fetch an epoch and its transactions, paginate from the front
  epoch1WithAfterCursorFromBack: epoch(epochId: 1) {
    totalTransactions
    transactions(last: 2, after: "@{cursor_0}") {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}

//# run-graphql --cursors 8
{ # Fetch an epoch and its transactions, paginate from the front
  epoch1WithBeforeCursorFromBack: epoch(epochId: 1) {
    totalTransactions
    transactions(last: 2, before: "@{cursor_0}") {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { effects { checkpoint { sequenceNumber } } } } 
    }
  }
}