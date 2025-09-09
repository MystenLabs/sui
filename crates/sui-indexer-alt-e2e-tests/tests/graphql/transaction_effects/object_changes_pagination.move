// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# programmable --sender A --inputs @A 42 43 44 45 46 47 48 49
//> 0: SplitCoins(Gas, [Input(1), Input(2), Input(3), Input(4)]);
//> 1: SplitCoins(Gas, [Input(5), Input(6), Input(7), Input(8)]);
//> 2: TransferObjects([NestedResult(0,0), NestedResult(0,1), NestedResult(0,2), NestedResult(0,3)], Input(0));
//> 3: TransferObjects([NestedResult(1,0), NestedResult(1,1), NestedResult(1,2), NestedResult(1,3)], Input(0))

//# create-checkpoint

//# run-graphql
{ # Get all the object changes
  transactionEffects(digest: "@{digest_1}") {
    objectChanges {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql
{ # Limit from the front
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(first: 3) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql
{ # Limit from the back
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(last: 3) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql --cursors 2
{ # Offset at the front and then fetch from the front
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(after: "@{cursor_0}", first: 3) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql --cursors 2
{ # Offset at the front and then fetch from the back
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(after: "@{cursor_0}", last: 3) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql --cursors 5
{ # Offset at the front such that the first page is not full
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(after: "@{cursor_0}", first: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql --cursors 5
{ # Offset at the front such that the first page is not full, fetch from the back
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(after: "@{cursor_0}", last: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql --cursors 5
{ # Offset at the back and fetch from the back
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(before: "@{cursor_0}", last: 3) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql --cursors 5
{ # Offset at the back and fetch from the front
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(before: "@{cursor_0}", first: 3) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql --cursors 2
{ # Offset at the back such that the first page is not full, fetch from the back
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(before: "@{cursor_0}", last: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql --cursors 2
{ # Offset at the back such that the first page is not full, fetch from the front
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(before: "@{cursor_0}", first: 5) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql --cursors 2 6
{ # Offset at the front and back
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(after: "@{cursor_0}", before: "@{cursor_1}") {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql --cursors 2 6
{ # Offset at the front and back, limit at the front
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(after: "@{cursor_0}", before: "@{cursor_1}", first: 2) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}

//# run-graphql --cursors 2 6
{ # Offset at the front and back, limit at the back
  transactionEffects(digest: "@{digest_1}") {
    objectChanges(after: "@{cursor_0}", before: "@{cursor_1}", last: 2) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }
      edges { cursor node { address } }
    }
  }
}
