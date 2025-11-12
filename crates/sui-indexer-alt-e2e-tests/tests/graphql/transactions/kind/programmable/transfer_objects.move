// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::transfer_test {
  public struct TestObject has key, store {
    id: UID,
    value: u64,
  }

  // Initialize with a test object transferred to account A
  fun init(ctx: &mut TxContext) {
    let test_obj = create_test_object(999, ctx);
    // Transfer to account A (first account in init)
    transfer::transfer(test_obj, @A);
  }

  // Simple function to create a test object
  public fun create_test_object(value: u64, ctx: &mut TxContext): TestObject {
    TestObject {
      id: object::new(ctx),
      value,
    }
  }
}

//# programmable --sender A --inputs 10 @B  
//> 0: test::transfer_test::create_test_object(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# create-checkpoint

//# programmable --sender A --inputs 10 object(1,0) @B  
//> 0: test::transfer_test::create_test_object(Input(0));
//> 1: TransferObjects([Result(0), Input(1)], Input(2));

//# create-checkpoint

//# run-graphql
{
  # Send a single Result object 
  sendSingleResultObject: transaction(digest: "@{digest_2}") {
    kind {
      ... on ProgrammableTransaction {
        commands(first: 10) {
          nodes {
            __typename
            ... on TransferObjectsCommand {
              inputs {
                __typename
                ... on Input { ix }
                ... on TxResult { cmd ix }
                ... on GasCoin { _ }
              }
              address {
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

//# run-graphql
{
  # Send multiple Result object 
  sendMultipleResultObjects: transaction(digest: "@{digest_4}") {
    kind {
      ... on ProgrammableTransaction {
        commands(first: 10) {
          nodes {
            __typename
            ... on TransferObjectsCommand {
              inputs {
                __typename
                ... on Input { ix }
                ... on TxResult { cmd ix }
                ... on GasCoin { _ }
              }
              address {
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
 