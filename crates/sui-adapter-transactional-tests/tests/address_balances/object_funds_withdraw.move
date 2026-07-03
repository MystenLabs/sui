// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Object-funds withdrawals with the in-execution sufficiency check enabled: a withdrawal within the
// object's settled balance succeeds, and an oversized one aborts in the VM
// (`funds_accumulator::E_OBJECT_FUNDS_INSUFFICIENT`).

//# init --addresses test=0x0 --accounts A --enable-feature-flags enable_accumulators --enable-feature-flags enable_object_funds_withdraw --enable-feature-flags check_object_funds_withdraw_in_execution

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

// Deposit `coin` into this vault's object balance (keyed by the vault's own address).
public fun fund(vault: &Vault, coin: Coin<SUI>) {
    balance::send_funds<SUI>(coin.into_balance(), vault.id.to_address());
}

// Withdraw `amount` from this vault's object balance and send it to `recipient`.
public fun withdraw_to(vault: &mut Vault, amount: u64, recipient: address) {
    let w = balance::withdraw_funds_from_object<SUI>(&mut vault.id, amount);
    let bal = balance::redeem_funds<SUI>(w);
    balance::send_funds<SUI>(bal, recipient);
}

// Create the vault, owned by A.
//# run test::obj_vault::new --sender A

// Fund the vault with 1000.
//# programmable --sender A --inputs 1000 object(2,0)
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::obj_vault::fund(Input(1), Result(0));

//# create-checkpoint

// The withdrawals are dry-run: an object-funds withdrawal is not sequenced by consensus in the
// transactional runner, so its accumulator version isn't assigned; the dry-run path instead pins the
// accumulator root at its latest version from the object store, which is what the in-execution check
// needs. Neither dry-run commits, so both see the same settled balance of 1000.

// Sufficient: withdraw 500 of the settled 1000. Succeeds.
//# programmable --sender A --dry-run --inputs object(2,0) 500 @A
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

// Insufficient: withdraw 5000 against a settled 1000. Aborts with E_OBJECT_FUNDS_INSUFFICIENT.
//# programmable --sender A --dry-run --inputs object(2,0) 5000 @A
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

// Cumulative within one transaction: 600 then 600 sums to 1200 > the settled 1000, so the second
// withdrawal aborts — the in-execution check counts the running total per object, not each
// withdrawal in isolation.
//# programmable --sender A --dry-run --inputs object(2,0) 600 @A
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));
//> 1: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

// A deposit earlier in the same transaction offsets the withdrawal: deposit 500 into the vault, then
// withdraw 1400. Against the settled 1000 alone the withdrawal is oversized, but the in-transaction
// deposit raises available funds, so the check compares against the net (1400 - 500 = 900 <= 1000)
// and succeeds.
//# programmable --sender A --dry-run --inputs 500 object(2,0) 1400 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::obj_vault::fund(Input(1), Result(0));
//> 2: test::obj_vault::withdraw_to(Input(1), Input(2), Input(3));

// Same shape, but the deposit does not fully cover the excess: deposit 500, withdraw 1600, net
// 1100 > the settled 1000, so it still aborts.
//# programmable --sender A --dry-run --inputs 500 object(2,0) 1600 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::obj_vault::fund(Input(1), Result(0));
//> 2: test::obj_vault::withdraw_to(Input(1), Input(2), Input(3));
