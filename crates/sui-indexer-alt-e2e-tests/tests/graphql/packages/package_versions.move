// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P1=0x0 P2=0x0  P3=0x0 --simulator

//# publish --upgradeable --sender A
module P1::M {
  public fun foo(): u64 { 42 }
}

//# create-checkpoint

//# upgrade --package P1 --upgrade-capability 1,1 --sender A
module P2::M {
  public fun foo(): u64 { 43 }
}

//# upgrade --package P2 --upgrade-capability 1,1 --sender A
module P3::M {
  public fun foo(): u64 { 44 }
}

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Fetch all package versions -- there should be 3 of these
  packageVersions(address: "@{P1}") {
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
      }
    }
  }
}

//# run-graphql
{ # Limit from the front
  packageVersions(address: "@{P1}", first: 2) {
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
      }
    }
  }
}

//# run-graphql
{ # Limit from the back
  packageVersions(address: "@{P1}", last: 2) {
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
      }
    }
  }
}

//# run-graphql --cursors 1
{ # Offset at the front and then fetch from the front
  packageVersions(address: "@{P1}", after: "@{cursor_0}", first: 1) {
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
      }
    }
  }
}

//# run-graphql --cursors 2
{ # Offset at the front such that the first page is not full
  packageVersions(address: "@{P1}", after: "@{cursor_0}", first: 2) {
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
      }
    }
  }
}

//# run-graphql --cursors 2
{ # Offset at the front such that the first page is not full, fetch from the back
  packageVersions(address: "@{P1}", after: "@{cursor_0}", last: 2) {
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
      }
    }
  }
}

//# run-graphql --cursors 3
{ # Offset at the back and fetch from the back
  packageVersions(address: "@{P1}", before: "@{cursor_0}", last: 1) {
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
      }
    }
  }
}

//# run-graphql --cursors 3
{ # Offset at the back and fetch from the front
  packageVersions(address: "@{P1}", before: "@{cursor_0}", first: 1) {
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
      }
    }
  }
}

//# run-graphql --cursors 2
{ # Offset at the back such that the first page is not full, fetch from the back
  packageVersions(address: "@{P1}", before: "@{cursor_0}", last: 2) {
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
      }
    }
  }
}

//# run-graphql --cursors 2
{ # Offset at the back such that the first page is not full, fetch from the front
  packageVersions(address: "@{P1}", before: "@{cursor_0}", first: 2) {
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
      }
    }
  }
}

//# run-graphql --cursors 1 3
{ # Offset at the front and back
  packageVersions(address: "@{P1}", after: "@{cursor_0}", before: "@{cursor_1}") {
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
      }
    }
  }
}

//# run-graphql
{ # Using packageVersions ..Before and ...After
  package(address: "@{P1}", version: 2) {
    packageVersionsBefore {
      nodes { version }
    }
    version
    packageVersionsAfter {
      nodes { version }
    }
  }
}

//# run-graphql
{ # Object is not a package
  packageVersions(address: "@{obj_5_0}") {
    nodes { version }
  }
}

//# run-graphql
{ # Anchor package doesn't exist at the checkpoint.
  doesntExist: checkpoint(sequenceNumber: 1) {
    query {
      packageVersions(address: "@{P2}") {
        nodes { version }
      }
    }
  }

  doesExist: checkpoint(sequenceNumber: 1) {
    query {
      packageVersions(address: "@{P1}") {
        nodes { version }
      }
    }
  }
}
