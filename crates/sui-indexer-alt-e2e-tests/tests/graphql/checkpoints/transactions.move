// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --simulator

//# run-graphql
{ # Fetch a checkpoint
  checkpoint(sequenceNumber: 0) {
    sequenceNumber
    digest
    transactions(first: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest, sender { address } } } 
    }
  }
}

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 43 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs 2
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs 3
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Fetch a checkpoint's transactions, should have sequence numbers 1,2,3,4,5
  checkpoint(sequenceNumber: 1) {
    sequenceNumber
    digest
    transactions(first: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest, sender { address } } } 
    }
  }
}

//# run-graphql --cursors 1
{ # Fetch a checkpoint's transactions, offset at the front
  checkpoint(sequenceNumber: 1) {
    sequenceNumber
    digest
    transactions( after: "@{cursor_0}", first: 3) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor }
    }
  }
}

//# run-graphql --cursors 1 5
{ # Fetch a checkpoint's transactions, offset from both ends and pick from the front
  checkpoint(sequenceNumber: 1) {
    sequenceNumber
    digest
    transactions( after: "@{cursor_0}", before: "@{cursor_1}", first: 2) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest, sender { address } } } 
    }
  }
}

//# run-graphql --cursors 1 5
{ # Fetch a checkpoint's transactions, offset from both ends and pick from the back
  checkpoint(sequenceNumber: 1) {
    sequenceNumber
    digest
    transactions( after: "@{cursor_0}", before: "@{cursor_1}", last: 2) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest, sender { address } } } 
    }
  }
}

//# run-graphql --cursors 3
{ # Fetch a checkpoint's transactions, offset from the end and pick from the back
  checkpoint(sequenceNumber: 1) {
    sequenceNumber
    digest
    transactions( before: "@{cursor_0}", last: 2) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest, sender { address } } } 
    }
  }
}

//# run-graphql --cursors 5
{ # Offset to a non-existent cursor
  checkpoint(sequenceNumber: 1) {
    sequenceNumber
    digest
    transactions( after: "@{cursor_0}") {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest, sender { address } } } 
    }
  }
}

//# run-graphql
{ # Fetch non-existent checkpoint
  checkpoint(sequenceNumber: 6) {
    sequenceNumber
    digest
    transactions(first: 1) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest, sender { address } } } 
    }
  }
}