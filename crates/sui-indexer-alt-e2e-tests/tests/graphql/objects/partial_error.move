// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test partial error behavior: when one field errors, other fields should still return data.
// This tests the Option<Result<T, E>> pattern for partial errors.

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Query an object with valid fields and an error field.
  # The response should have both data (for successful fields) AND errors (for failed field)
  object(address: "@{obj_0_0}") {
    version
    digest
    # Error field
    objectAt(version: 1, rootVersion: 2) {
      version
    }
  }
}

//# run-graphql
{
  # Partial error at the query root level with multiple objects
  # One succeeds, one fails - both should be in the response
  validObject: object(address: "@{obj_0_0}") {
    version
    digest
  }

  invalidObjectAt: object(address: "@{obj_0_0}") {
    objectAt(version: 1, checkpoint: 1) {
      version
    }
  }
}
