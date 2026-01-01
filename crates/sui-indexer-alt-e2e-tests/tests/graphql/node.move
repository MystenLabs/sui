// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# run-graphql --cursors bcs(0u8,@{A})
{ # Fetch an address
  node(id: "@{cursor_0}") {
    id
    ... on Address {
      address
      objects {
        nodes {
          contents {
            type { repr }
            json
          }
        }
      }
    }
  }
}

//# run-graphql --cursors bcs(1u8,0)
{ # Fetch an epoch
  node(id: "@{cursor_0}") {
    id
    ... on Epoch {
      epochId
      startTimestamp
    }
  }
}

//# run-graphql --cursors bcs(2u8,0x2) bcs(3u8,0x2)
{ # Fetch a package
  package: node(id: "@{cursor_0}") {
    id
    ... on MovePackage {
      modules(first: 3) {
        nodes {
          name
        }
      }
    }
  }

  object: node(id: "@{cursor_1}") {
    id
    ... on Object {
      asMovePackage {
        modules(first: 3) {
          nodes {
            name
          }
        }
      }
    }
  }
}

//# run-graphql --cursors bcs(3u8,@{obj_0_0})
{ # Fetch an object
  node(id: "@{cursor_0}") {
    id
    ... on Object {
      asMoveObject {
        contents {
          type { repr }
          json
        }
      }
    }
  }
}
