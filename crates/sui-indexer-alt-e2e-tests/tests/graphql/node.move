// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# run-graphql --cursors bcs(0u8,@{A})
{
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
