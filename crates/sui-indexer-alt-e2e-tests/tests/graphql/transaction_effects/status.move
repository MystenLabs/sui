// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::failed_module {
    /// Function that always aborts, creating a failed transaction
    public entry fun always_fails() {
        abort 42
    }
}

//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# run test::failed_module::always_fails --sender A

//# create-checkpoint

//# run-graphql
{ # Test status field on successful transaction effects
  successTransaction: transaction(digest: "@{digest_2}") {
    digest
    effects {
      status
    }
  }
} 

//# run-graphql
{ # Test status field on failed transaction
  failedTransaction: transaction(digest: "@{digest_3}") {
    digest
    effects {
      status
    }
  }
}
