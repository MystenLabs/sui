// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags check_object_funds_withdraw_in_execution

//# publish --sender A
module test::obj_vault;

use sui::balance;
use sui::coin::Coin;
use sui::sui::SUI;

public struct Vault has key {
    id: UID,
}

public fun new(ctx: &mut TxContext) {
    transfer::transfer(Vault { id: object::new(ctx) }, ctx.sender());
}

public fun fund(vault: &Vault, coin: Coin<SUI>) {
    balance::send_funds<SUI>(coin.into_balance(), vault.id.to_address());
}

public fun withdraw_to(vault: &mut Vault, amount: u64, recipient: address) {
    let w = balance::withdraw_funds_from_object<SUI>(&mut vault.id, amount);
    let bal = balance::redeem_funds<SUI>(w);
    balance::send_funds<SUI>(bal, recipient);
}

//# run test::obj_vault::new --sender A

//# programmable --sender A --inputs 1000 object(2,0)
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::obj_vault::fund(Input(1), Result(0));

//# create-checkpoint

//# programmable --sender A --dry-run --inputs object(2,0) 500 @A
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

//# programmable --sender A --dry-run --inputs object(2,0) 5000 @A
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

//# programmable --sender A --dry-run --inputs object(2,0) 600 @A
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));
//> 1: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

//# programmable --sender A --dry-run --inputs 500 object(2,0) 1400 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::obj_vault::fund(Input(1), Result(0));
//> 2: test::obj_vault::withdraw_to(Input(1), Input(2), Input(3));

//# programmable --sender A --dry-run --inputs 500 object(2,0) 1600 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::obj_vault::fund(Input(1), Result(0));
//> 2: test::obj_vault::withdraw_to(Input(1), Input(2), Input(3));

//# programmable --sender A --dry-run --inputs object(2,0) 0 @A
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

//# run test::obj_vault::new --sender A

//# create-checkpoint

//# programmable --sender A --dry-run --inputs 700 object(11,0) 600 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::obj_vault::fund(Input(1), Result(0));
//> 2: test::obj_vault::withdraw_to(Input(1), Input(2), Input(3));

//# run test::obj_vault::new --sender A

//# programmable --sender A --inputs 1000 object(14,0)
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::obj_vault::fund(Input(1), Result(0));

//# create-checkpoint

//# programmable --sender A --inputs object(14,0) 600 @A
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

//# programmable --sender A --inputs object(14,0) 600 @A
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

//# create-checkpoint

//# programmable --sender A --inputs object(14,0) 400 @A
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

//# programmable --sender A --inputs 500 object(14,0) 400 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::obj_vault::fund(Input(1), Result(0));
//> 2: test::obj_vault::withdraw_to(Input(1), Input(2), Input(3));
