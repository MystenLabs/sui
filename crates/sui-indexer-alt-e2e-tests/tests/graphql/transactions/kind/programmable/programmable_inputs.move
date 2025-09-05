// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::simple {
  use sui::transfer::Receiving;

  public struct Counter has key {
    id: UID,
    value: u64,
  }

  public struct Coin has key, store {
    id: UID,
    value: u64,
  }

  fun init(ctx: &mut TxContext) {
    transfer::share_object(Counter {
        id: object::new(ctx),
        value: 0,
    });
  }

  public fun add(counter: &mut Counter, amount: u64) {
    counter.value = counter.value + amount;
  }

  public fun new_coin(value: u64, ctx: &mut TxContext): Coin {
    Coin { id: object::new(ctx), value }
  }

  public fun get_value(coin: &Coin): u64 {
    coin.value
  }

  public fun transfer_coin_to_address(coin: Coin, to_address: address) {
    transfer::transfer(coin, to_address)
  }

  public fun receive_coin(receiver: &mut Coin, incoming: Receiving<Coin>): Coin {
    transfer::receive(&mut receiver.id, incoming)
  }
}

// Test 1: Pure input only - demonstrates Pure TransactionInput type
//# programmable --sender A --inputs 42u64 @A
//> 0: test::simple::new_coin(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

// Test 2: SharedInput only - demonstrates SharedInput TransactionInput type
//# programmable --sender A --inputs object(1,0) 10  
//> 0: test::simple::add(Input(0), Input(1))

//# create-checkpoint

// Test 3: OwnedOrImmutable only - demonstrates OwnedOrImmutable TransactionInput type
// Reuses the coin created in Test 1 (object(2,0))
//# programmable --sender A --inputs object(2,0)
//> 0: test::simple::get_value(Input(0))

//# create-checkpoint

// Test 4: Receiving only - demonstrates Receiving TransactionInput type
//# programmable --sender A --inputs 200u64 @A  
// Setup: Create coin and transfer it to A's address for Receiving test
//> 0: test::simple::new_coin(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(8,0) @A
// Setup: Transfer coin to A's address (creates a pending receive)
//> 0: test::simple::transfer_coin_to_address(Input(0), Input(1))

//# programmable --sender A --inputs object(2,0) receiving(8,0) @A
//> 0: test::simple::receive_coin(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# create-checkpoint

//# run-graphql
{
  # Test 1: Pure input only
  pureInputTest: transaction(digest: "@{digest_2}") {
    digest
    kind {
      __typename
      ... on ProgrammableTransaction {
        inputs(first: 10) {
          nodes {
            __typename
            ... on Pure {
              bytes
            }
          }
        }
      }
    }
  }
}

//# run-graphql  
{
  # Test 2: SharedInput only
  sharedInputTest: transaction(digest: "@{digest_4}") {
    digest
    kind {
      __typename
      ... on ProgrammableTransaction {
        inputs(first: 10) {
          nodes {
            __typename
            ... on SharedInput {
              address
              initialSharedVersion
              mutable
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # Test 3: OwnedOrImmutable input only  
  ownedOrImmutableTest: transaction(digest: "@{digest_6}") {
    digest
    kind {
      __typename
      ... on ProgrammableTransaction {
        inputs(first: 10) {
          nodes {
            __typename
            ... on OwnedOrImmutable {
              object {
                address
                version
                digest
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
  # Test 4: Receiving input only
  receivingInputTest: transaction(digest: "@{digest_10}") {
    digest
    kind {
      __typename
      ... on ProgrammableTransaction {
        inputs(first: 10) {
          nodes {
            __typename
            ... on Receiving {
              object {
                address
                version
                digest
              }
            }
          }
        }
      }
    }
  }
} 