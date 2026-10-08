// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Deposits to forwarding-shaped addresses that cannot resolve fail the transaction instead of
// stranding funds. Resolution happens at the end of execution, so every failure here is an
// execution error without a command index, not a Move abort.
// The first registration is assigned master ID 0x52ca8647179c (LE bytes 9c174786ca52).

//# init --addresses test=0x0 --accounts A B C

//# publish --sender B
module test::thing;

public struct Thing has key, store {
    id: UID,
}

public fun send(recipient: address, ctx: &mut TxContext) {
    transfer::public_transfer(Thing { id: object::new(ctx) }, recipient);
}

//# programmable --sender B --inputs mutshared(250) 1000 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101 --gas-budget 2000000000
// Registering and depositing in one transaction: resolution reads the registry at the version
// assigned to the transaction, so it does not see the new record and the transaction fails.
// The registration is rolled back with the rest of the transaction.
//> 0: sui::forwarding_address::register(Input(0));
//> 1: TransferObjects([Result(0)], Input(2));
//> 2: SplitCoins(Gas, [Input(1)]);
//> 3: sui::coin::send_funds<sui::sui::SUI>(Result(2), Input(2));

//# programmable --sender A --inputs mutshared(250) @A --gas-budget 2000000000
// Still the first master ID, since the failed registration allocated nothing.
//> 0: sui::forwarding_address::register(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender B --inputs 1000 @0x010000000000fafafafafafafafafa0001010101010101010101010101010101
// Unregistered master ID.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs 1000 @0x9c174786ca52fafafafafafafafafa0101010101010101010101010101010101
// Registered master ID with variant 1, above forwarding_address_max_variant.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender C --inputs @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101 --gas-budget 10000000
// Sending the gas coin itself to a forwarding address fails, even though the address is
// registered: the master would net the coin's value minus gas, which no ForwardingDeposit can
// state. Payers split the amount off the gas coin instead.
//> sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0))

//# programmable --sender B --inputs 1000 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101
// Transferring a coin object to a forwarding address fails: objects are not rerouted yet.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender B --inputs @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101
// So does transferring the gas coin itself.
//> TransferObjects([Gas], Input(0));

//# programmable --sender B --inputs @0x010000000000fafafafafafafafafa0001010101010101010101010101010101
// And any other object, from Move, whether or not the forwarding address is registered.
//> test::thing::send(Input(0));

//# programmable --sender B --inputs 1000 @0x00000000000000fafafafafafafafafa00010101010101010101010101010101
// Magic at the wrong offset is an ordinary address and is credited directly.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
