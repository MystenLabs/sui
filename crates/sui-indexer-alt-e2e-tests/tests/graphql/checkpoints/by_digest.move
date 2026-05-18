// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --accounts A --simulator

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 1
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Resolve checkpoints by digest, using the framework-injected `cp_digest_N` substitutions.
  c1: checkpoint(digest: "@{cp_digest_1}") {
    sequenceNumber
    digest
  }
  c2: checkpoint(digest: "@{cp_digest_2}") {
    sequenceNumber
    digest
  }
}

//# run-graphql
{ # An unknown digest returns null without erroring.
  missing: checkpoint(digest: "11111111111111111111111111111111") {
    sequenceNumber
  }
}

//# run-graphql
{ # Specifying both `sequenceNumber` and `digest` is a user error.
  both: checkpoint(sequenceNumber: 1, digest: "@{cp_digest_1}") {
    sequenceNumber
  }
}
