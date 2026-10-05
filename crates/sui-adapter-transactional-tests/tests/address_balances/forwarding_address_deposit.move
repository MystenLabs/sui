// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A registers a master ID and B deposits to a forwarding address under it: the funds land in A's
// address balance, the forwarding address stays empty, and the deposit emits ForwardingDeposit.
// The registry (0xfa) is a mutable input to register, and an implicit read for deposits.
// The first registration is assigned master ID 0x688990c0, so the forwarding address with
// variant 0 and payload 0x01 x 17 is 0xc0908968 fafafafafafafafafafa 00 0101...01.

//# init --addresses test=0x0 --accounts A B

//# programmable --sender A --inputs mutshared(250) @A --gas-budget 2000000000
// Registration pays the registration fee.
//> 0: sui::forwarding_address::register(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender B --inputs 1000 @0xc0908968fafafafafafafafafafa000101010101010101010101010101010101
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs 2000 @0xc0908968fafafafafafafafafafa000202020202020202020202020202020202
// A different payload under the same master routes to the same master.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
