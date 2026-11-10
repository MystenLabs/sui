// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The frozen B transfers objects whose coin sits outside their own bytes to C, and C takes the
// coin out. Freezing the holding objects' IDs blocks this.

//# init --accounts A B C --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::bag::{Self, Bag};
use sui::balance::{Self, Balance};
use sui::coin::Coin;
use sui::coin_registry;
use sui::dynamic_field as df;
use sui::dynamic_object_field as dof;
use sui::table::{Self, Table};
use sui::transfer::Receiving;

public struct REGULATED_COIN has drop {}

public struct Vault has key, store {
    id: UID,
}

public struct BagVault has key, store {
    id: UID,
    bag: Bag,
}

public struct TableVault has key, store {
    id: UID,
    table: Table<u64, Balance<REGULATED_COIN>>,
}

public struct Wrapper has key, store {
    id: UID,
    balance: Balance<REGULATED_COIN>,
}

fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    let (mut currency, mut treasury_cap) = coin_registry::new_currency_with_otw(
        otw,
        9,
        b"RC".to_string(),
        b"REGULATED_COIN".to_string(),
        b"A new regulated coin".to_string(),
        b"".to_string(),
        ctx,
    );
    let cap = currency.make_compliance_regulated(true, ctx);
    let metadata_cap = currency.finalize(ctx);
    let coin = treasury_cap.mint(10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(cap, ctx.sender());
    transfer::public_transfer(metadata_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
}

public fun df_vault(coin: Coin<REGULATED_COIN>, ctx: &mut TxContext): Vault {
    let mut id = object::new(ctx);
    df::add(&mut id, 0u64, coin.into_balance());
    Vault { id }
}

public fun dof_vault(coin: Coin<REGULATED_COIN>, ctx: &mut TxContext): Vault {
    let mut id = object::new(ctx);
    dof::add(&mut id, 0u64, coin);
    Vault { id }
}

public fun bag_vault(coin: Coin<REGULATED_COIN>, ctx: &mut TxContext): BagVault {
    let id = object::new(ctx);
    let mut bag = bag::new(ctx);
    bag.add(0u64, coin.into_balance());
    BagVault { id, bag }
}

public fun table_vault(coin: Coin<REGULATED_COIN>, ctx: &mut TxContext): TableVault {
    let id = object::new(ctx);
    let mut table = table::new(ctx);
    table.add(0u64, coin.into_balance());
    TableVault { id, table }
}

public fun account_vault(coin: Coin<REGULATED_COIN>, ctx: &mut TxContext): Vault {
    let id = object::new(ctx);
    coin.send_funds(id.to_address());
    Vault { id }
}

public fun receive_vault(coin: Coin<REGULATED_COIN>, ctx: &mut TxContext): Vault {
    let id = object::new(ctx);
    transfer::public_transfer(coin, id.to_address());
    Vault { id }
}

public fun wrapper(coin: Coin<REGULATED_COIN>, ctx: &mut TxContext): Wrapper {
    Wrapper { id: object::new(ctx), balance: coin.into_balance() }
}

public fun take_df(vault: &mut Vault, ctx: &mut TxContext): Coin<REGULATED_COIN> {
    let balance: Balance<REGULATED_COIN> = df::remove(&mut vault.id, 0u64);
    balance.into_coin(ctx)
}

public fun take_dof(vault: &mut Vault): Coin<REGULATED_COIN> {
    dof::remove(&mut vault.id, 0u64)
}

public fun take_bag(vault: &mut BagVault, ctx: &mut TxContext): Coin<REGULATED_COIN> {
    let balance: Balance<REGULATED_COIN> = vault.bag.remove(0u64);
    balance.into_coin(ctx)
}

public fun take_table(vault: &mut TableVault, ctx: &mut TxContext): Coin<REGULATED_COIN> {
    vault.table.remove(0u64).into_coin(ctx)
}

public fun take_account(vault: &mut Vault, amount: u64, ctx: &mut TxContext): Coin<REGULATED_COIN> {
    let withdrawal = balance::withdraw_funds_from_object<REGULATED_COIN>(&mut vault.id, amount);
    balance::redeem_funds(withdrawal).into_coin(ctx)
}

public fun take_receive(
    vault: &mut Vault,
    coin: Receiving<Coin<REGULATED_COIN>>,
): Coin<REGULATED_COIN> {
    transfer::public_receive(&mut vault.id, coin)
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

// Each holding place gets 100 and goes to B before the freeze.
//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: test::regulated_coin::df_vault(Result(0));
//> TransferObjects([Result(1)], Input(2))

//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: test::regulated_coin::dof_vault(Result(0));
//> TransferObjects([Result(1)], Input(2))

//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: test::regulated_coin::bag_vault(Result(0));
//> TransferObjects([Result(1)], Input(2))

//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: test::regulated_coin::table_vault(Result(0));
//> TransferObjects([Result(1)], Input(2))

//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: test::regulated_coin::account_vault(Result(0));
//> TransferObjects([Result(1)], Input(2))

//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: test::regulated_coin::receive_vault(Result(0));
//> TransferObjects([Result(1)], Input(2))

// Control: the Balance is in the object's own bytes.
//# programmable --sender A --inputs object(1,5) 100 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: test::regulated_coin::wrapper(Result(0));
//> TransferObjects([Result(1)], Input(2))

//# create-checkpoint

//# view-object 3,0

//# view-object 3,1

//# view-object 4,0

//# view-object 4,1

//# view-object 4,2

//# view-object 5,0

//# view-object 5,1

//# view-object 6,0

//# view-object 6,1

//# view-object 7,0

//# view-object 8,0

//# view-object 8,1

//# view-object 9,0

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

// Control: the frozen sender transfers the Wrapper.
//# programmable --sender B --inputs object(9,0) @C
//> TransferObjects([Input(0)], Input(1))

// Dynamic field holding a Balance.
//# programmable --sender B --inputs object(3,0) @C
//> TransferObjects([Input(0)], Input(1))

// Dynamic object field holding a Coin.
//# programmable --sender B --inputs object(4,1) @C
//> TransferObjects([Input(0)], Input(1))

// Bag entry holding a Balance.
//# programmable --sender B --inputs object(5,0) @C
//> TransferObjects([Input(0)], Input(1))

// Table entry holding a Balance.
//# programmable --sender B --inputs object(6,0) @C
//> TransferObjects([Input(0)], Input(1))

// Address balance of the object's ID.
//# programmable --sender B --inputs object(7,0) @C
//> TransferObjects([Input(0)], Input(1))

// Coin sent to the object's ID and not received.
//# programmable --sender B --inputs object(8,1) @C
//> TransferObjects([Input(0)], Input(1))

// The new owner takes the coin out of each.
//# programmable --sender C --inputs object(3,0) @C
//> 0: test::regulated_coin::take_df(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender C --inputs object(4,1) @C
//> 0: test::regulated_coin::take_dof(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender C --inputs object(5,0) @C
//> 0: test::regulated_coin::take_bag(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender C --inputs object(6,0) @C
//> 0: test::regulated_coin::take_table(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender C --inputs object(7,0) 100 @C
//> 0: test::regulated_coin::take_account(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender C --inputs object(8,1) receiving(8,0) @C
//> 0: test::regulated_coin::take_receive(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender C --inputs object(32,0) object(4,0) object(34,0) object(35,0) object(36,0) object(8,0)
//> MergeCoins(Input(0), [Input(1), Input(2), Input(3), Input(4), Input(5)])

//# view-object 32,0
