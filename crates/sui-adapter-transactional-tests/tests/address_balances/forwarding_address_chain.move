// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A chain: B's master id points at one of A's forwarding addresses, so a deposit to B's
// forwarding address passes through two hops to A, with one ForwardingDeposit per hop, each
// recorded in the stream of that hop's master.
// A is assigned master id 0x52ca8647179c (LE 9c174786ca52) and B 0x2b339572355c (LE 5c357295332b).

//# init --addresses test=0x0 --accounts A B C

//# programmable --sender A --inputs mutshared(250) @A 1 --gas-budget 2000000000
//> 0: sui::forwarding_address::register(Input(0), Input(2));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender B --inputs mutshared(250) @0x9c174786ca52fafafafafafafafafa00aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa @B 1 --gas-budget 2000000000
// B registers with A's forwarding address (payload aa..aa) as master.
//> 0: sui::forwarding_address::register_with_forwarding_master(Input(0), Input(1), Input(3));
//> 1: TransferObjects([Result(0)], Input(2));

//# programmable --sender C --inputs 1000 @0x5c357295332bfafafafafafafafafa00bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
// Paying B's forwarding address (payload bb..bb) credits A after two hops.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# programmable --sender C --inputs 500 @0x5c357295332bfafafafafafafafafa00bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
// A coin object takes the same path.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender B --inputs mutshared(250) @0x2d2d975175e0fafafafafafafafafa00cccccccccccccccccccccccccccccccc @B 1 --gas-budget 2000000000
// An id cannot point at its own forwarding address: the next id allocated is 0xe07551972d2d.
//> 0: sui::forwarding_address::register_with_forwarding_master(Input(0), Input(1), Input(3));
//> 1: TransferObjects([Result(0)], Input(2));

//# programmable --sender B --inputs mutshared(250) @0x9c174786ca52fafafafafafafafafa01aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa @B 1 --gas-budget 2000000000
// Nor at a forwarding address with an unsupported variant.
//> 0: sui::forwarding_address::register_with_forwarding_master(Input(0), Input(1), Input(3));
//> 1: TransferObjects([Result(0)], Input(2));

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
