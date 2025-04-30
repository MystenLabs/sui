// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Fetch all object versions -- there should be 6 of these.
  objectVersions(address: "@{obj_0_0}") {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    edges {
      cursor
      node {
        address
        version
        objectBcs
      }
    }
  }
}

//# run-graphql
{ # Limit from the front
  objectVersions(address: "@{obj_0_0}", first: 3) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql
{ # Limit from the back
  objectVersions(address: "@{obj_0_0}", last: 3) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql --cursors 1
{ # Offset at the front and then fetch from the front
  objectVersions(address: "@{obj_0_0}", after: "@{cursor_0}", first: 3) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql --cursors 4
{ # Offset at the front such that the first page is not full
  objectVersions(address: "@{obj_0_0}", after: "@{cursor_0}", first: 5) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql --cursors 4
{ # Offset at the front such that the first page is not full, fetch from the back
  objectVersions(address: "@{obj_0_0}", after: "@{cursor_0}", last: 5) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql --cursors 5
{ # Offset at the back and fetch from the back
  objectVersions(address: "@{obj_0_0}", before: "@{cursor_0}", last: 3) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql --cursors 5
{ # Offset at the back and fetch from the front
  objectVersions(address: "@{obj_0_0}", before: "@{cursor_0}", first: 3) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql --cursors 2
{ # Offset at the back such that the first page is not full, fetch from the back
  objectVersions(address: "@{obj_0_0}", before: "@{cursor_0}", last: 5) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql --cursors 2
{ # Offset at the back such that the first page is not full, fetch from the front
  objectVersions(address: "@{obj_0_0}", before: "@{cursor_0}", first: 5) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql --cursors 1 4
{ # Offset at the front and back
  objectVersions(address: "@{obj_0_0}", after: "@{cursor_0}", before: "@{cursor_1}") {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql --cursors 1 4
{ # Offset at the front and back, limit at the front
  objectVersions(address: "@{obj_0_0}", after: "@{cursor_0}", before: "@{cursor_1}", first: 1) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql --cursors 1 4
{ # Offset at the front and back, limit at the back
  objectVersions(address: "@{obj_0_0}", after: "@{cursor_0}", before: "@{cursor_1}", last: 1) {
    pageInfo {
      hasPreviousPage
      hasNextPage
      startCursor
      endCursor
    }
    nodes { version }
  }
}

//# run-graphql
{ # Using objectVersions ...Before and ...After
  object(address: "@{obj_0_0}", version: 3) {
    objectVersionsBefore {
      nodes { version }
    }
    version
    objectVersionsAfter {
      nodes { version }
    }
  }
}
