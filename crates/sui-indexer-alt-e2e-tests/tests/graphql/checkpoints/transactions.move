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

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Fetch a checkpoint
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

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Fetch a checkpoint
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

//# run-graphql --cursors {"t":1}
{ # Fetch a checkpoint, offset at the front and fetch from the front
  checkpoint(sequenceNumber: 1) {
    sequenceNumber
    digest
    transactions(first: 2, after: "@{cursor_0}") {
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

//# run-graphql --cursors {"t":1}
{ # Fetch a checkpoint, and transactions before tx_sequence_number 1 and fetch from the back, should be empty
  checkpoint(sequenceNumber: 1) {
    sequenceNumber
    digest
    transactions(last: 2, before: "@{cursor_0}") {
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

//# run-graphql --cursors {"t":1} {"t":3}
{ # Fetch a checkpoint, and transactions after tx_sequence_number 1 and before 3 and fetch from the front
  checkpoint(sequenceNumber: 1) {
    sequenceNumber
    digest
    transactions(first: 3, after: "@{cursor_0}", before: "@{cursor_1}") {
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
{ # Fetch a non-existent checkpoint
  checkpoint(sequenceNumber: 3) {
    sequenceNumber
    digest
    transactions(first: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { digest } }
    }
  }
}