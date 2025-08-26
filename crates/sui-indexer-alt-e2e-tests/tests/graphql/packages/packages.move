// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// P1 is obj(1,0)
// P2 is obj(3,0)
// P3 is obj(6,0)
// Q1 is obj(4,0)
// Q2 is obj(8,0)
// Q3 is obj(9,0)

//# init --protocol-version 70 --accounts A --addresses P1=0x0 P2=0x0 P3=0x0 Q1=0x0 Q2=0x0 Q3=0x0 --simulator

//# publish --upgradeable --sender A
module P1::M {
	public fun foo(): u64 { 42 }
}

//# create-checkpoint

//# upgrade --package P1 --upgrade-capability 1,1 --sender A
module P2::M {
	public fun foo(): u64 { 43 }
}

//# publish --upgradeable --sender A
module Q1::N {
  public fun bar(): u64 { 44 }
}

//# create-checkpoint

//# upgrade --package P2 --upgrade-capability 1,1 --sender A
module P3::M {
  public fun foo(): u64 { 45 }
}

//# create-checkpoint

//# upgrade --package Q1 --upgrade-capability 4,1 --sender A
module Q2::N {
  public fun bar(): u64 { 46 }
}

//# upgrade --package Q2 --upgrade-capability 4,1 --sender A
module Q3::N {
  public fun bar(): u64 { 47 }
}

//# create-checkpoint

//# run-graphql
{ # Fetch the packages published to map package names to addresses
  p1: package(address: "@{P1}", version: 1) { address }
  p2: package(address: "@{P2}", version: 2) { address }
  p3: package(address: "@{P3}", version: 3) { address }
  q1: package(address: "@{Q1}", version: 1) { address }
  q2: package(address: "@{Q2}", version: 2) { address }
  q3: package(address: "@{Q3}", version: 3) { address }
}

//# run-graphql
{ # Fetch all packages
  packages {
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
  packages(first: 2) {
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
  packages(last: 2) {
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

//# run-graphql --cursors bcs(1,bcs(@{obj_1_0}),1)
{ # Offset at the front and then fetch from the front
  packages(after: "@{cursor_0}", first: 1) {
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

//# run-graphql --cursors bcs(4,bcs(@{obj_4_0}),2)
{ # Offset at the front such that the first page is not full
  packages(after: "@{cursor_0}", first: 2) {
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

//# run-graphql --cursors bcs(4,bcs(@{obj_4_0}),2)
{ # Offset at the front such that the first page is not full, and then fetch from the back
  packages(after: "@{cursor_0}", last: 2) {
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

//# run-graphql --cursors bcs(4,bcs(@{obj_4_0}),2)
{ # Offset at the back and fetch from the back
  packages(before: "@{cursor_0}", last: 1) {
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

//# run-graphql --cursors bcs(4,bcs(@{obj_4_0}),2)
{ # Offset at the back and fetch from the front
  packages(before: "@{cursor_0}", first: 1) {
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

//# run-graphql --cursors bcs(0,bcs(0x2),1)
{ # Offset at the back such that the first page is not full, fetch from the back
  packages(before: "@{cursor_0}", last: 2) {
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

//# run-graphql --cursors bcs(0,bcs(0x2),1)
{ # Offset at the back such that the first page is not full, fetch from the front
  packages(before: "@{cursor_0}", first: 2) {
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

//# run-graphql --cursors bcs(1,bcs(@{obj_1_0}),1) bcs(4,bcs(@{obj_4_0}),2)
{ # Offset at the front and back
  packages(after: "@{cursor_0}", before: "@{cursor_1}") {
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
{ # Using filter
  packages(filter: { afterCheckpoint: 1, beforeCheckpoint: 4 }) {
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
{ # Time travel query
  checkpoint(sequenceNumber: 3) {
    query {
      packages {
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
  }

}
