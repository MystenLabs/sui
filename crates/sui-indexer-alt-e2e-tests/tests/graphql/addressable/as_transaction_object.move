// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --accounts A --simulator

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  # Address present in the tx as a newly created object → ObjectChange variant.
  changed: address(address: "@{obj_1_0}") {
    asTransactionObject(transactionDigest: "@{digest_1}") {
      __typename
      ... on ObjectChange {
        idCreated
        outputState { address }
      }
    }
  }

  # No `transactionDigest` argument outside subscription scope → null.
  noArg: address(address: "@{obj_1_0}") {
    asTransactionObject {
      __typename
    }
  }

  # Address not referenced by the tx → null.
  notReferenced: address(
    address: "0x000000000000000000000000000000000000000000000000000000000000dead"
  ) {
    asTransactionObject(transactionDigest: "@{digest_1}") {
      __typename
    }
  }
}
