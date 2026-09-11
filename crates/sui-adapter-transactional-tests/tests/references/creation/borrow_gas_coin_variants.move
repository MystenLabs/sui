// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Gas paid from an address balance is an ephemeral coin whose Move value holds only the budget,
// and gas paid with a coin plus a balance reservation is a smashed coin.

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs 100000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# programmable --sender A --address-balance-gas --gas-budget 10000000 --inputs 1000 @B
// INVALID: InsufficientCoinBalance at command 1, the ephemeral gas coin holds only the budget so nothing can be split through the reference
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: SplitCoins(Result(0), [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(1));

//# view-object 4,0

//# programmable --sender A --address-balance-gas --gas-budget 10000000 --inputs @B
// VALID: the ephemeral gas coin withdrawn through a `&mut Balance` reference yields an empty coin
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Gas);
//> 1: sui::balance::withdraw_all<sui::sui::SUI>(Result(0));
//> 2: sui::coin::from_balance<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(2)], Input(0));

//# view-object 6,0

//# programmable --sender A --address-balance-gas --gas-budget 10000000 --inputs @B
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, the ephemeral gas coin sent while borrowed
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0));
//> 2: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));

//# programmable --sender A --gas-payment object(0,0) --gas-payment withdraw<sui::balance::Balance<sui::sui::SUI>>(500000000) --inputs 1000 @B
// VALID: a reference into the smashed gas coin when a coin and a reservation pay for gas
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: SplitCoins(Result(0), [Input(0)]);
//> 2: TransferObjects([Result(1)], Input(1));

//# view-object 9,0

//# create-checkpoint

//# view-funds sui::balance::Balance<sui::sui::SUI> A
