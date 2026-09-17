// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Minimal app for exercising app-bound allowances in e2e tests.
module move_test_code::allowance_app;

use sui::allowance::{Self, Allowance, AllowanceProposal, AllowanceWithdrawal};
use sui::balance::Balance;
use sui::clock::Clock;

public struct APP has drop {}

public fun issue<T>(proposal: AllowanceProposal<T>, ctx: &mut TxContext) {
    allowance::issue(proposal, allowance::settings_permit(internal::permit<APP>()), ctx)
}

public fun rotate<T>(a: &mut Allowance<T>, new_spender: address) {
    allowance::rotate_spender(a, allowance::settings_permit(internal::permit<APP>()), new_spender)
}

public fun spend<C>(
    a: &mut Allowance<Balance<C>>,
    w: AllowanceWithdrawal<Balance<C>>,
    clock: &Clock,
    ctx: &TxContext,
): Balance<C> {
    allowance::app_balance_spend(a, allowance::spend_permit(internal::permit<APP>()), w, clock, ctx)
}
