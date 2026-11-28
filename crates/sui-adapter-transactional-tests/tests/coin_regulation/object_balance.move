// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Withdrawing from an object's balance and depositing it back persists no balance-bearing object,
// but still touches the coin, so it fails while the object is frozen or the coin is paused.

//# init --accounts A --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::balance;
use sui::coin::Coin;
use sui::coin_registry;
use sui::coin_regulation::{ComplianceCap, FreezeList};

public struct REGULATED_COIN has drop {}

public struct Vault has key {
    id: UID,
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

public fun new_vault(ctx: &mut TxContext) {
    transfer::transfer(Vault { id: object::new(ctx) }, ctx.sender());
}

public fun fund(vault: &Vault, coin: Coin<REGULATED_COIN>) {
    coin.send_funds(vault.id.to_address());
}

public fun withdraw_and_redeposit(vault: &mut Vault, amount: u64) {
    let withdrawal = balance::withdraw_funds_from_object<REGULATED_COIN>(&mut vault.id, amount);
    balance::redeem_funds(withdrawal).send_funds(vault.id.to_address());
}

public fun freeze_vault(
    list: &mut FreezeList,
    cap: &mut ComplianceCap<REGULATED_COIN>,
    vault: &Vault,
    ctx: &mut TxContext,
) {
    list.freeze_address(cap, vault.id.to_address(), ctx);
}

public fun unfreeze_vault(
    list: &mut FreezeList,
    cap: &mut ComplianceCap<REGULATED_COIN>,
    vault: &Vault,
    ctx: &mut TxContext,
) {
    list.unfreeze_address(cap, vault.id.to_address(), ctx);
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run test::regulated_coin::new_vault --sender A

//# programmable --sender A --inputs object(3,0) object(1,5) 100
//> 0: SplitCoins(Input(1), [Input(2)]);
//> 1: test::regulated_coin::fund(Input(0), Result(0));

//# create-checkpoint

//# run test::regulated_coin::withdraw_and_redeposit --args object(3,0) 10 --sender A

//# run test::regulated_coin::freeze_vault --args object(0xcf) object(1,3) object(3,0) --sender A

//# run test::regulated_coin::withdraw_and_redeposit --args object(3,0) 10 --sender A

//# run test::regulated_coin::unfreeze_vault --args object(0xcf) object(1,3) object(3,0) --sender A

//# run sui::coin_regulation::enable_global_pause --args object(0xcf) object(1,3) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run test::regulated_coin::withdraw_and_redeposit --args object(3,0) 10 --sender A

//# run sui::coin_regulation::disable_global_pause --args object(0xcf) object(1,3) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run test::regulated_coin::withdraw_and_redeposit --args object(3,0) 10 --sender A
