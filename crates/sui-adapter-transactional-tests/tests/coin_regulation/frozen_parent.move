// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A freeze of an object's ID blocks its coin while that ID is the coin's direct parent. A Bag
// moved to a new object sheds a freeze of the outer object's ID but not a freeze of its own ID.

//# init --accounts A B C --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::bag::{Self, Bag};
use sui::balance::Balance;
use sui::coin::Coin;
use sui::coin_registry;
use sui::coin_regulation::{ComplianceCap, FreezeList};
use sui::dynamic_field as df;

public struct REGULATED_COIN has drop {}

public struct Vault has key, store {
    id: UID,
}

public struct BagVault has key, store {
    id: UID,
    bag: Bag,
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

public fun bag_vault(coin: Coin<REGULATED_COIN>, ctx: &mut TxContext): BagVault {
    let id = object::new(ctx);
    let mut bag = bag::new(ctx);
    bag.add(0u64, coin.into_balance());
    BagVault { id, bag }
}

public fun take_df(vault: &mut Vault, ctx: &mut TxContext): Coin<REGULATED_COIN> {
    let balance: Balance<REGULATED_COIN> = df::remove(&mut vault.id, 0u64);
    balance.into_coin(ctx)
}

public fun take_bag(vault: &mut BagVault, ctx: &mut TxContext): Coin<REGULATED_COIN> {
    let balance: Balance<REGULATED_COIN> = vault.bag.remove(0u64);
    balance.into_coin(ctx)
}

public fun rewrap_bag(vault: BagVault, ctx: &mut TxContext): BagVault {
    let BagVault { id, bag } = vault;
    id.delete();
    BagVault { id: object::new(ctx), bag }
}

public fun freeze_object<O: key>(
    list: &mut FreezeList,
    cap: &mut ComplianceCap<REGULATED_COIN>,
    obj: &O,
    ctx: &mut TxContext,
) {
    list.freeze_address(cap, object::id_address(obj), ctx);
}

public fun freeze_bag(
    list: &mut FreezeList,
    cap: &mut ComplianceCap<REGULATED_COIN>,
    vault: &BagVault,
    ctx: &mut TxContext,
) {
    list.freeze_address(cap, object::id_address(&vault.bag), ctx);
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(1,5) 100 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: test::regulated_coin::df_vault(Result(0));
//> TransferObjects([Result(1)], Input(2))

//# programmable --sender A --inputs object(1,5) 100 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: test::regulated_coin::bag_vault(Result(0));
//> TransferObjects([Result(1)], Input(2))

//# programmable --sender A --inputs object(1,5) 100 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: test::regulated_coin::bag_vault(Result(0));
//> TransferObjects([Result(1)], Input(2))

// Freeze B, the ID of the first two objects, and the ID of the Bag inside the third.
//# programmable --sender A --inputs object(0xcf) object(1,3) object(3,0) object(4,0) object(5,0) @B
//> 0: test::regulated_coin::freeze_object<test::regulated_coin::Vault>(Input(0), Input(1), Input(2));
//> 1: test::regulated_coin::freeze_object<test::regulated_coin::BagVault>(Input(0), Input(1), Input(3));
//> 2: test::regulated_coin::freeze_bag(Input(0), Input(1), Input(4));
//> 3: sui::coin_regulation::freeze_address<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(5));

// The objects reach the frozen B and leave it again.
//# programmable --sender A --inputs object(3,0) object(4,0) object(5,0) @B
//> TransferObjects([Input(0), Input(1), Input(2)], Input(3))

//# programmable --sender B --inputs object(3,0) object(4,0) object(5,0) @C
//> TransferObjects([Input(0), Input(1), Input(2)], Input(3))

// Frozen object ID that is the parent of the dynamic field.
//# programmable --sender C --inputs object(3,0) @C
//> 0: test::regulated_coin::take_df(Input(0));
//> TransferObjects([Result(0)], Input(1))

// Frozen object ID around an unfrozen Bag.
//# programmable --sender C --inputs object(4,0) @C
//> 0: test::regulated_coin::take_bag(Input(0));
//> TransferObjects([Result(0)], Input(1))

// Unfrozen object ID around a frozen Bag.
//# programmable --sender C --inputs object(5,0) @C
//> 0: test::regulated_coin::take_bag(Input(0));
//> TransferObjects([Result(0)], Input(1))

// The unfrozen Bag moves to an object with a new ID.
//# programmable --sender C --inputs object(4,0) @C
//> 0: test::regulated_coin::rewrap_bag(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender C --inputs object(12,0) @C
//> 0: test::regulated_coin::take_bag(Input(0));
//> TransferObjects([Result(0)], Input(1))

// The frozen Bag moves to an object with a new ID.
//# programmable --sender C --inputs object(5,0) @C
//> 0: test::regulated_coin::rewrap_bag(Input(0));
//> TransferObjects([Result(0)], Input(1))

//# programmable --sender C --inputs object(14,0) @C
//> 0: test::regulated_coin::take_bag(Input(0));
//> TransferObjects([Result(0)], Input(1))
