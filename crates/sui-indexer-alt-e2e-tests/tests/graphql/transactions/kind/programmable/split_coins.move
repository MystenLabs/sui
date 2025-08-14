// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# programmable --sender A --inputs 100 200 @B  
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: TransferObjects([NestedResult(0, 0), NestedResult(0, 1)], Input(2));

//# create-checkpoint

//# programmable --sender A --inputs 50 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([NestedResult(0, 0)], Input(1));

//# create-checkpoint

//# run-graphql
{
  # Split Gas with multiple amounts 
  splitMultipleAmounts: transaction(digest: "@{digest_1}") {
    kind {
      ... on ProgrammableTransaction {
        commands {
          nodes {
            __typename
            ... on SplitCoinsCommand {
              coin { ...Arg }
              amounts { ...Arg }
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
}

//# run-graphql
{ 
  # Split Gas with single amount
  splitSingleAmount: transaction(digest: "@{digest_3}") {
    kind {
      ... on ProgrammableTransaction {
        commands {
          nodes {
            __typename
            ... on SplitCoinsCommand {
              coin {
                __typename
                ... on Input { ix }
                ... on TxResult { cmd ix }
                ... on GasCoin { _ }
              }
              amounts {
                __typename
                ... on Input { ix }
                ... on TxResult { cmd ix }
                ... on GasCoin { _ }
              }
            }
          }
        }
      }
    }
  }
} 