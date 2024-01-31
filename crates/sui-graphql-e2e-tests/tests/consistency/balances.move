// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// cp | coins
// ------------
// 1  | (300, 100, 200) = 600
// 2  | (400, 100, 200) = 700
// 3  | (400, 100)      = 500
// 4  | (400)           = 400
// 7  | (400)

// snapshot@[0, 2), all transaction blocks will have valid data.
// snapshot@[0, 3), first transaction block is out of available range.
// snapshot@[0, 4), first two transaction blocks are out of available range.
// snapshot@[0, 6), all transaction blocks are out of available range.

//# init --addresses P0=0x0 --accounts A B --simulator

//# publish --sender A
module P0::fake {
    use std::option;
    use sui::coin;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext){
        let (treasury_cap, metadata) = coin::create_currency(
            witness,
            2,
            b"FAKE",
            b"",
            b"",
            option::none(),
            ctx,
        );

        let c1 = coin::mint(&mut treasury_cap, 100, ctx);
        let c2 = coin::mint(&mut treasury_cap, 200, ctx);
        let c3 = coin::mint(&mut treasury_cap, 300, ctx);

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
        transfer::public_transfer(c1, tx_context::sender(ctx));
        transfer::public_transfer(c2, tx_context::sender(ctx));
        transfer::public_transfer(c3, tx_context::sender(ctx));
    }
}

//# create-checkpoint

//# programmable --sender A --inputs object(1,5) 100 object(1,1)
//> 0: sui::coin::mint<P0::fake::FAKE>(Input(0), Input(1));
//> MergeCoins(Input(2), [Result(0)]);

//# create-checkpoint

//# transfer-object 1,2 --sender A --recipient B

//# create-checkpoint

//# transfer-object 1,3 --sender A --recipient B

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql
{
  transactionBlocks(filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
        }
        allBalances: balances {
          nodes {
            coinType {
              repr
            }
            coinObjectCount
            totalBalance
          }
        }
      }
    }
  }
}


//# force-object-snapshot-catchup --start-cp 0 --end-cp 2

//# run-graphql
{
  transactionBlocks(filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
        }
        allBalances: balances {
          nodes {
            coinType {
              repr
            }
            coinObjectCount
            totalBalance
          }
        }
      }
    }
  }
}


//# force-object-snapshot-catchup --start-cp 0 --end-cp 3

//# run-graphql
{
  transactionBlocks(filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
        }
        allBalances: balances {
          nodes {
            coinType {
              repr
            }
            coinObjectCount
            totalBalance
          }
        }
      }
    }
  }
}


//# force-object-snapshot-catchup --start-cp 0 --end-cp 4

//# run-graphql
{
  transactionBlocks(filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
        }
        allBalances: balances {
          nodes {
            coinType {
              repr
            }
            coinObjectCount
            totalBalance
          }
        }
      }
    }
  }
}


//# force-object-snapshot-catchup --start-cp 0 --end-cp 6

//# run-graphql
{
  transactionBlocks(filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
        }
        allBalances: balances {
          nodes {
            coinType {
              repr
            }
            coinObjectCount
            totalBalance
          }
        }
      }
    }
  }
}
