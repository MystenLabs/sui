// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Deposits to forwarding-shaped addresses that cannot resolve abort instead of stranding funds.
// The first registration is assigned master ID 0x688990c0 (LE bytes c0908968).

//# init --addresses test=0x0 --accounts A B

//# programmable --sender B --inputs mutshared(250) 1000 @0xc0908968fafafafafafafafafafa000101010101010101010101010101010101 --gas-budget 2000000000
// Registering and depositing in one transaction: resolution reads the registry at the version
// assigned to the transaction, so it does not see the new record and aborts as unregistered (1).
// The registration is rolled back with the rest of the transaction.
//> 0: sui::forwarding_address::register(Input(0));
//> 1: TransferObjects([Result(0)], Input(2));
//> 2: SplitCoins(Gas, [Input(1)]);
//> 3: sui::coin::send_funds<sui::sui::SUI>(Result(2), Input(2));

//# programmable --sender A --inputs mutshared(250) @A --gas-budget 2000000000
// Still the first master ID, since the aborted registration allocated nothing.
//> 0: sui::forwarding_address::register(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender B --inputs 1000 @0x01000000fafafafafafafafafafa000101010101010101010101010101010101
// Unregistered master ID: aborts with code 1.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs 1000 @0xc0908968fafafafafafafafafafa010101010101010101010101010101010101
// Registered master ID with variant 1, above forwarding_address_max_variant: aborts with code 2.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs @0xc0908968fafafafafafafafafafa000101010101010101010101010101010101 --gas-budget 10000000
// Sending the gas coin itself to a forwarding address is rejected, even when it would resolve.
//> sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0))

//# programmable --sender B --inputs 1000 @0x0000000000fafafafafafafafafafa0001010101010101010101010101010101
// Magic at the wrong offset is an ordinary address and is credited directly.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
