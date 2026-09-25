// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::balance::Balance;
use sui::funds_accumulator::Withdrawal;
use sui::sui::SUI;

public fun id_mut<T>(t: &mut T): &mut T { t }
public fun limit_at_least(w: &Withdrawal<Balance<SUI>>, v: u256) {
    assert!(w.withdrawal_limit() >= v, 0)
}
public fun use_mut<T>(_: &mut T) {}

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs withdraw<sui::balance::Balance<sui::sui::SUI>>(500) @B 500u256
// VALID: reference into the withdrawal, then redeemed by value once the reference is dead
//> 0: test::m::id_mut<sui::funds_accumulator::Withdrawal<sui::balance::Balance<sui::sui::SUI>>>(Input(0));
//> 1: test::m::limit_at_least(Result(0), Input(2));
//> 2: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 3: sui::balance::send_funds<sui::sui::SUI>(Result(2), Input(1));

//# programmable --sender A --inputs withdraw<sui::balance::Balance<sui::sui::SUI>>(100) @B
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, redeemed while borrowed
//> 0: test::m::id_mut<sui::funds_accumulator::Withdrawal<sui::balance::Balance<sui::sui::SUI>>>(Input(0));
//> 1: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 2: test::m::use_mut<sui::funds_accumulator::Withdrawal<sui::balance::Balance<sui::sui::SUI>>>(Result(0));
