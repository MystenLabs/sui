// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::move_call_test {
  use sui::coin::{Self, Coin};
  use sui::sui::SUI;

  public struct TestObject has drop {
    value: u64,
  }

  // Simple function that takes pure value - demonstrates Input arguments
  public fun create_test_object(value: u64): TestObject {
    TestObject {
      value,
    }
  }

  // Function that takes an object - demonstrates Result arguments
  public fun get_object_value(obj: &TestObject): u64 {
    obj.value
  }

  // Function that takes gas coin by reference - demonstrates GasCoin arguments
  public fun check_gas_coin(coin: &Coin<SUI>): u64 {
    coin::value(coin)
  }

  // Function for testing nested results - transfer coins to avoid drop issues
  public fun transfer_coins(coin1: Coin<SUI>, coin2: Coin<SUI>, recipient: address) {
    transfer::public_transfer(coin1, recipient);
    transfer::public_transfer(coin2, recipient);
  }
}

//# programmable --sender A --inputs 42u64 1000u64 @A @B
//> 0: test::move_call_test::create_test_object(Input(0));
//> 1: test::move_call_test::get_object_value(Result(0));
//> 2: test::move_call_test::check_gas_coin(Gas);
//> 3: SplitCoins(Gas, [Input(0), Input(0)]);
//> 4: test::move_call_test::transfer_coins(NestedResult(3,0), NestedResult(3, 1), Input(2));

//# create-checkpoint

//# run-graphql
{
  transaction(digest: "@{digest_2}") {
    kind {
      ... on ProgrammableTransaction {
        commands(first: 10) {
          nodes {
            ... on MoveCallCommand {
              function {
                module {
                  package { address }
                  name
                }
                name
              }
              arguments {
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
