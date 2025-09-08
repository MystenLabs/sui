// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses Test=0x0 --accounts A --simulator

//# run-graphql
{
  invalid_kind_affected_address: transactions(filter: { kind: PROGRAMMABLE_TX, affectedAddress: "@{A}" }) {
    edges {
      cursor
    }
  }
}
