// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// App-bound allowances through a published third-party module: proposal-based
// issuance, spends gated on the app's permit, plain spends rejected, spender
// rotation flipping the sign-time check, and a withdrawal composed through an
// ordinary public function.

//# init --accounts A B C --addresses test=0x0

//# publish --sender A
module test::app;

use sui::allowance::{Self, Allowance, AllowanceProposal, AllowanceWithdrawal};
use sui::balance::{Self, Balance};
use sui::clock::Clock;

public struct APP has drop {}

public fun issue<T>(proposal: AllowanceProposal<T>, ctx: &mut TxContext) {
    allowance::issue(proposal, allowance::settings_permit(internal::permit<APP>()), ctx)
}

public fun rotate<T>(a: &mut Allowance<T>, new_spender: address) {
    allowance::rotate_spender(a, allowance::settings_permit(internal::permit<APP>()), new_spender)
}

/// The app's payment flow: spend and forward in one call.
public fun app_pay<C>(
    a: &mut Allowance<Balance<C>>,
    w: AllowanceWithdrawal<Balance<C>>,
    clock: &Clock,
    recipient: address,
    ctx: &TxContext,
) {
    let b = allowance::app_balance_spend(a, allowance::spend_permit(internal::permit<APP>()), w, clock, ctx);
    balance::send_funds(b, recipient)
}

/// Third-party composition over a plain allowance: the withdrawal is an
/// ordinary argument.
public fun forward_plain<C>(
    a: &mut Allowance<Balance<C>>,
    w: AllowanceWithdrawal<Balance<C>>,
    clock: &Clock,
    recipient: address,
    ctx: &TxContext,
) {
    let b = allowance::balance_spend(a, w, clock, ctx);
    balance::send_funds(b, recipient)
}

//# programmable --sender A --inputs 5000 @A
// Fund A's (the funder) address balance.
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Result(0), Input(1));

//# create-checkpoint

//# programmable --sender A --inputs b"app" @B vector[1000u256] vector[] vector[99999999999999]
// A proposes an app-bound allowance to B and the app issues it.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::propose_for_app<sui::balance::Balance<sui::sui::SUI>, test::app::APP>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));
//> 2: test::app::issue<sui::balance::Balance<sui::sui::SUI>>(Result(1));

//# programmable --sender A --inputs b"plain" @B vector[1000u256] vector[] vector[99999999999999]
// A issues a plain allowance to B, for the composed spend below.
//> 0: std::option::none<sui::allowance::RateLimit>();
//> 1: sui::allowance::new<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1), Input(2), Input(3), Input(4), Result(0));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(400,@A,object(4,0)) mutshared(4,0) immshared(6) @B
// B spends the app-bound allowance through the app.
//> 0: test::app::app_pay<sui::sui::SUI>(Input(1), Input(0), Input(2), Input(3));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(100,@A,object(4,0)) mutshared(4,0) immshared(6) @B
// A plain spend on the app-bound allowance: aborts.
//> 0: sui::allowance::balance_spend<sui::sui::SUI>(Input(1), Input(0), Input(2));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(3));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(250,@A,object(5,0)) mutshared(5,0) immshared(6) @B
// B spends the plain allowance through the third-party function.
//> 0: test::app::forward_plain<sui::sui::SUI>(Input(1), Input(0), Input(2), Input(3));

//# programmable --sender A --inputs mutshared(4,0) @C
// The app rotates the spender to C.
//> 0: test::app::rotate<sui::balance::Balance<sui::sui::SUI>>(Input(0), Input(1));

//# programmable --sender B --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(100,@A,object(4,0)) mutshared(4,0) immshared(6) @B
// The old spender after the rotation: rejected at signing.
//> 0: test::app::app_pay<sui::sui::SUI>(Input(1), Input(0), Input(2), Input(3));

//# programmable --sender C --inputs allowance_withdraw<sui::balance::Balance<sui::sui::SUI>>(300,@A,object(4,0)) mutshared(4,0) immshared(6) @C
// The new spender works.
//> 0: test::app::app_pay<sui::sui::SUI>(Input(1), Input(0), Input(2), Input(3));

//# view-object 4,0
// spender is C; current_spend is 700: 400 + 300, untouched by the rejections.

//# view-object 5,0
// current_spend is 250.

//# create-checkpoint

//# view-object 2,0
// A's settled balance: 5000 - 400 - 250 - 300 = 4050.

//# view-object 6,0
// B's settled balance: 400 + 250 = 650.

//# view-object 11,0
// C's settled balance: 300.
