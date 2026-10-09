// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Mixes forwarding-address deposits with object funds withdrawals in one transaction, so the
// accumulator root and the forwarding registry (0xfa) are both implicitly read, and the registry
// is also a mutable or immutable input in some transactions.
// A's master ID is 0x52ca8647179c, so FA = 0x9c174786ca52 fafafafafafafafafa 00 0101...01 forwards to A.
// B's registration below gets the second ID, 0x2b339572355c; a third would get 0xe07551972d2d, so
// FC = 0x2d2d975175e0 fafafafafafafafafa 00 0101...01.

//# init --addresses test=0x0 --accounts A B --enable-feature-flags check_object_funds_withdraw_in_execution

//# publish --sender B
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

//# run test::obj_vault::new --sender B

//# programmable --sender B --inputs 1000 object(2,0)
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::obj_vault::fund(Input(1), Result(0));

//# programmable --sender A --inputs mutshared(250) @A 1 --gas-budget 2000000000
//> 0: sui::forwarding_address::register(Input(0), Input(2));
//> 1: TransferObjects([Result(0)], Input(1));

//# create-checkpoint

//# programmable --sender B --inputs object(2,0) 100 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101
// Object funds withdrawn from the vault and deposited to FA reach A.
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

//# programmable --sender B --inputs mutshared(250) @B object(2,0) 100 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101 1 --gas-budget 2000000000
// B registers, then withdraws from the vault and deposits to FA. The registry is a mutated input
// and also read for resolution at the same version; A's record was committed earlier, so it
// resolves.
//> 0: sui::forwarding_address::register(Input(0), Input(5));
//> 1: TransferObjects([Result(0)], Input(1));
//> 2: test::obj_vault::withdraw_to(Input(2), Input(3), Input(4));

//# programmable --sender B --inputs mutshared(250) @B object(2,0) 100 @0x2d2d975175e0fafafafafafafafafa0001010101010101010101010101010101 1 --gas-budget 2000000000
// B registers again and deposits vault funds to the id this registration gets (FC). Resolution
// does not see the new record, so the transaction fails and the withdrawal
// is rolled back with it.
//> 0: sui::forwarding_address::register(Input(0), Input(5));
//> 1: TransferObjects([Result(0)], Input(1));
//> 2: test::obj_vault::withdraw_to(Input(2), Input(3), Input(4));

//# programmable --sender B --inputs mutshared(250) object(2,0) 100 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101
// The registry as an unused mutable input: the deposit still resolves through it.
//> 0: test::obj_vault::withdraw_to(Input(1), Input(2), Input(3));

//# programmable --sender B --inputs immshared(250) object(2,0) 100 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101
// The registry as an unused immutable input together with its implicit read.
//> 0: test::obj_vault::withdraw_to(Input(1), Input(2), Input(3));

//# programmable --sender B --inputs object(2,0) 100 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101 5000 @A
// The forwarded deposit succeeds, then a second withdrawal exceeds the vault balance, so the
// whole transaction fails and the forwarded deposit does not land.
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));
//> 1: test::obj_vault::withdraw_to(Input(0), Input(3), Input(4));

//# programmable --sender B --inputs object(2,0) 100 @0x010000000000fafafafafafafafafa0001010101010101010101010101010101
// Vault funds sent to an unregistered forwarding address fail instead of stranding.
//> 0: test::obj_vault::withdraw_to(Input(0), Input(1), Input(2));

//# programmable --sender B --inputs 300 object(2,0) 800 @0x9c174786ca52fafafafafafafafafa0002020202020202020202020202020202
// Funding the vault and withdrawing more than its settled balance in one transaction, forwarded
// to another FA payload: the in-transaction deposit counts toward the withdrawal.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::obj_vault::fund(Input(1), Result(0));
//> 2: test::obj_vault::withdraw_to(Input(1), Input(2), Input(3));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
