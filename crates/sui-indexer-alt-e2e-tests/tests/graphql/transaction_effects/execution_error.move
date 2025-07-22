// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::execution_error_tests {
    /// Function that always aborts with code 42
    public entry fun abort_with_42() {
        abort 42
    }

    /// Function that always aborts with code 255  
    public entry fun abort_with_255() {
        abort 255
    }
}

//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# run test::execution_error_tests::abort_with_42 --sender A

//# run test::execution_error_tests::abort_with_255 --sender B

//# create-checkpoint

//# run-graphql
{
  # Test execution_error on successful transaction (should be null)
  successTransaction: transactionEffects(digest: "@{digest_2}") {
    executionError {
      message
      moveAbortCode
    }
  }

  # Test execution_error on Move abort with code 42 
  moveAbort42: transactionEffects(digest: "@{digest_3}") {
    executionError {
      message
      moveAbortCode
    }
  }

  # Test execution_error on Move abort with code 255
  moveAbort255: transactionEffects(digest: "@{digest_4}") {
    executionError {
      message  
      moveAbortCode
    }
  }

  # Test execution_error combined with other fields
  combinedFields: transactionEffects(digest: "@{digest_3}") {
    status
    executionError {
      message
      moveAbortCode
    }
    lamportVersion
  }
} 