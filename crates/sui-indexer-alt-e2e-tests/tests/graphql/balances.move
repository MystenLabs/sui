// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Simple e2e test of coin, address, and total balance queries.

//# init --protocol-version 108 --accounts A B --addresses T=0x0 --simulator --enable-accumulators

//# publish --sender A
#[allow(deprecated_usage)]
module T::test {
    use sui::coin::{Self, TreasuryCap};
    use sui::balance;

    public struct TEST has drop {}

    fun init(otw: TEST, ctx: &mut TxContext){
        let (treasury, metadata) = coin::create_currency(otw, 6, b"", b"", b"", option::none(), ctx);
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, ctx.sender());
    }

    // Create A_COINS using the TreasuryCap.
    public fun mint_coin(
        treasury_cap: &mut TreasuryCap<TEST>,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        let coin = coin::mint(treasury_cap, amount, ctx);
        transfer::public_transfer(coin, recipient)
    }

    // Mint to address balance
    public fun mint_balance(
        treasury_cap: &mut TreasuryCap<TEST>,
        amount: u64,
        recipient: address,
        ctx: &mut TxContext,
    ) {
        balance::send_funds(coin::into_balance(coin::mint(treasury_cap, amount, ctx)), recipient);
    }
}

//# view-object 1,1

//# programmable --sender A --inputs object(1,1) 1000000u64 2000u64 @A
//> T::test::mint_balance(Input(0), Input(1), Input(3));
//> T::test::mint_coin(Input(0), Input(2), Input(3));

//# create-checkpoint

//# run-graphql
{
    address(address: "@{A}") {
        balance(coinType: "@{T}::test::TEST") {
            totalBalance
            coinBalance
            addressBalance
        }
        balances {
            nodes {
                coinType {
                    repr
                }
                totalBalance
                coinBalance
                addressBalance
            }
        }
    }
}
