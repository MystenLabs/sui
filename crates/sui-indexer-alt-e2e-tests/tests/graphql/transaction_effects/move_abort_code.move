// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::abort_tests {
    /// Function that always aborts with code 42
    public entry fun abort_with_42() {
        abort 42
    }
}

//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# run test::abort_tests::abort_with_42 --sender A

//# create-checkpoint

//# run-graphql
{ # Test move_abort_code field on successful transaction (should be null)
  successTransaction: transaction(digest: "@{digest_2}") {
    digest
    effects {
      moveAbortCode
    }
  }
} 

//# run-graphql
{ # Test move_abort_code field on failed transaction with abort code 42
  abortCode42: transaction(digest: "@{digest_3}") {
    digest
    effects {
      moveAbortCode
    }
  }
} 