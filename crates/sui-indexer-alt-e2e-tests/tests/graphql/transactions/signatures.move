// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B C --simulator

//# programmable --sender A --inputs 42 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender B --sponsor C --inputs 43 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{  # Test single signature transaction
  singleSig: transaction(digest: "@{digest_1}") {
    digest
    signatures {
      signatureBytes
    }
  }
}

//# run-graphql
{ # Test multiple signature transaction (sender + sponsor)
  multiSig: transaction(digest: "@{digest_2}") {
    digest
    signatures {
      signatureBytes
    }
  }
}
