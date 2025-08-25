// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# programmable --sender A --inputs 100 200 @B  
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: MergeCoins(Gas, [NestedResult(0, 0)]);
//> 2: TransferObjects([NestedResult(0, 1)], Input(2));

//# create-checkpoint

//# programmable --sender A --inputs 50 75
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: MergeCoins(NestedResult(0, 0), [NestedResult(0, 1)]);

//# create-checkpoint

//# run-graphql
{
  # Merge one coin back into Gas coin
  mergeIntoGas: transaction(digest: "@{digest_1}") {
    kind {
      ... on ProgrammableTransaction {
        commands(first: 10) {
          nodes {
            __typename
            ... on MergeCoinsCommand {
              coin { ...Arg }
              coins { ...Arg }
            }
          }
        }
      }
    }
  }
}

fragment Arg on TransactionArgument {
  __typename
  ... on Input { ix }
  ... on TxResult { cmd ix }
  ... on GasCoin { _ }
}


//# run-graphql
{ 
  # Merge split coins together
  mergeSplitCoins: transaction(digest: "@{digest_3}") {
    kind {
      ... on ProgrammableTransaction {
        commands(first: 10) {
          nodes {
            __typename
            ... on MergeCoinsCommand {
              coin { ...Arg }
              coins { ...Arg }
            }
          }
        }
      }
    }
  }
} 

fragment Arg on TransactionArgument {
  __typename
  ... on Input { ix }
  ... on TxResult { cmd ix }
  ... on GasCoin { _ }
}
