// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests custom types introduced by an upgrade in legacy coin reservations and
// allowance-backed withdrawals. Covers synthesized calls in empty PTBs, mixed input
// kinds, dependency-version conflicts, and original command-index attribution.
// Unused reservations preserve balances, while an allowance spend transfers exactly seven units.

//# init --addresses DepV1=0x0 DepV2=0x0 Consumer=0x0 --accounts A B

//# publish --upgradeable --sender A
module DepV1::m;
public fun check(expected: u64) { assert!(expected == 1, 11) }

//# publish --dependencies DepV1 --sender A
module Consumer::m;
public fun check(expected: u64) { DepV1::m::check(expected) }

//# upgrade --package DepV1 --upgrade-capability 1,1 --sender A
module DepV2::m;
public struct New has drop {}
public struct Mint has key { id: UID, supply: sui::balance::Supply<New> }
public fun check(expected: u64) { assert!(expected == 2, 22) }
public fun fund(ctx: &mut TxContext) {
    let mut supply = sui::balance::create_supply(New {});
    sui::balance::send_funds(supply.increase_supply(100), ctx.sender());
    transfer::transfer(Mint { id: object::new(ctx), supply }, ctx.sender())
}

//# run DepV2::m::fund --sender A

//# create-checkpoint

//# view-funds sui::balance::Balance<DepV2::m::New> A

//# programmable --sender A --inputs coin_reservation<sui::balance::Balance<DepV2::m::New>>(5)
// The custom package is supplied only by the input; every command is synthesized.

//# create-checkpoint

//# view-funds sui::balance::Balance<DepV2::m::New> A
// Redemption and send-back leave the original 100 units unchanged.

//# programmable --sender A --inputs coin_reservation<sui::balance::Balance<DepV2::m::New>>(5) 2u64
// An unused legacy reservation still selects v2 for the ordinary consumer.
//> Consumer::m::check(Input(1));

//# programmable --sender A --inputs coin_reservation<sui::balance::Balance<DepV2::m::New>>(5) 1u64
// The same input conflicts with an exact v1 target before conversions execute.
//> DepV1::m::check(Input(1));

//# programmable --sender A --inputs 2u64 object(4,0) withdraw<sui::balance::Balance<DepV2::m::New>>(5) coin_reservation<sui::balance::Balance<DepV2::m::New>>(5) @A
// Only the legacy input is converted to Coin; explicit withdrawals retain their input kind.
//> Consumer::m::check(Input(0));
//> sui::coin::value<DepV2::m::New>(Input(3));
//> 2: sui::balance::redeem_funds<DepV2::m::New>(Input(2));
//> sui::balance::send_funds<DepV2::m::New>(Result(2), Input(4));

//# programmable --sender A --inputs coin_reservation<sui::balance::Balance<DepV2::m::New>>(5) 2u64 99u64
// The inserted redemption must not shift this abort's original command index from 1 to 2.
//> Consumer::m::check(Input(1));
//> Consumer::m::check(Input(2));

//# create-checkpoint

//# view-funds sui::balance::Balance<DepV2::m::New> A
// Successful round trips and the failed transaction have not consumed the 100 units.

//# programmable --sender A --inputs b"custom" @B vector[100u256] vector[] vector[99999999999999]
// Grant B an allowance over the custom type introduced in v2.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<DepV2::m::New>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<DepV2::m::New>>(5,@A,object(16,0)) immshared(16,0) 2u64
// The unused allowance-backed inputs resolve the custom type without spending it.
//> Consumer::m::check(Input(2));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<DepV2::m::New>>(5,@A,object(16,0)) immshared(16,0) 1u64
// An allowance source has the same minimum-version constraint as a sender withdrawal.
//> DepV1::m::check(Input(2));

//# view-object 16,0
// Unused and conflicting declarations have not changed current_spend from zero.

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<DepV2::m::New>>(7,@A,object(16,0)) mutshared(16,0) immshared(6) @B 2u64
// A used AllowanceWithdrawal instantiates its custom wrapper and spends exactly seven units.
//> Consumer::m::check(Input(4));
//> 1: sui::allowance::balance_spend<DepV2::m::New>(Input(1), Input(0), Input(2));
//> sui::balance::send_funds<DepV2::m::New>(Result(1), Input(3));

//# create-checkpoint

//# view-funds sui::balance::Balance<DepV2::m::New> A
// The funder now has 93 units.

//# view-funds sui::balance::Balance<DepV2::m::New> B
// The recipient now has seven units.

//# view-object 16,0
// current_spend is seven, with no spending from the unused declarations.
