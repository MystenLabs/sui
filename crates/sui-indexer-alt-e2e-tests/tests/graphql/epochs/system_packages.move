// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# advance-epoch

//# run-graphql
{ # Check what the system packages are from genesis.
  epoch(epochId: 0) {
    systemPackages {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }

      nodes {
        address
        version
      }
    }
  }
}

//# run-graphql
{ # System packages for the latest epoch, which should be the same as for
  # genesis.
  epoch {
    systemPackages {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }

      nodes {
        address
        version
      }
    }
  }
}

//# run-graphql
{ # Limit from the front
  epoch {
    systemPackages(first: 2) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }

      nodes {
        address
        version
      }
    }
  }
}

//# run-graphql
{ # Limit from the back
  epoch {
    systemPackages(last: 2) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }

      nodes {
        address
        version
      }
    }
  }
}

//# run-graphql --cursors bcs(0x2)
{ # Offset at the front and limit
  epoch {
    systemPackages(after: "@{cursor_0}", first: 2) {
      pageInfo {
        hasPreviousPage
        hasNextPage
        startCursor
        endCursor
      }

      nodes {
        address
        version
      }
    }
  }
}
