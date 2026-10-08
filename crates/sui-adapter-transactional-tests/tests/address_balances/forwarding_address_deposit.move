// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A registers a master ID and B deposits to a forwarding address under it: the funds land in A's
// address balance, the forwarding address stays empty, and the deposit emits ForwardingDeposit.
// The registry (0xfa) is a mutable input to register, and an implicit read for deposits.
// The first registration is assigned master ID 0x52ca8647179c, so the forwarding address with
// variant 0 and payload 0x01 x 16 is 0x9c174786ca52 fafafafafafafafafa 00 0101...01.

//# init --addresses test=0x0 --accounts A B

//# programmable --sender A --inputs mutshared(250) @A --gas-budget 2000000000
// Registration pays the registration fee.
//> 0: sui::forwarding_address::register(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender B --inputs 1000 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender B --inputs 2000 @0x9c174786ca52fafafafafafafafafa0002020202020202020202020202020202
// A different payload under the same master routes to the same master.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# programmable --sender B --inputs 300 400 @0x9c174786ca52fafafafafafafafafa0001010101010101010101010101010101 @0x9c174786ca52fafafafafafafafafa0002020202020202020202020202020202
// Two deposits to one forwarding address in one transaction emit a single ForwardingDeposit for
// their sum (700); a deposit to another forwarding address gets its own event.
//> 0: SplitCoins(Gas, [Input(0), Input(1), Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(NestedResult(0, 0), Input(2));
//> 2: sui::coin::send_funds<sui::sui::SUI>(NestedResult(0, 1), Input(2));
//> 3: sui::coin::send_funds<sui::sui::SUI>(NestedResult(0, 2), Input(3));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
