// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Query address with valid fields and an error field (invalid pagination).
  # The response should have both data (for successful fields) AND errors (for failed field)
  address(address: "@{A}") {
    # Valid field
    address
    # Error field - first and last together is invalid
    objects(first: 1, last: 1) {
      nodes {
        address
      }
    }
  }
}

//# run-graphql
{
  # Test partial error at query root level
  # One query succeeds with all fields, another has partial error
  validAddress: address(address: "@{A}") {
    address
  }

  partialErrorAddress: address(address: "@{A}") {
    address
    # Error field - first and last together is invalid
    balances(first: 1, last: 1) {
      nodes {
        coinType { repr }
      }
    }
  }
}
