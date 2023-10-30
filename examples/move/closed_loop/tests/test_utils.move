// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This module defines base testing utilities for the
module closed_loop::test_utils {
    use sui::coin::{Self, TreasuryCap};
    use sui::tx_context::{dummy, TxContext};
    use closed_loop::closed_loop::{Self, Token, TokenPolicy, TokenPolicyCap};

    /// The type of the test Token.
    struct TEST has drop {}

    /// Get a context for testing.
    public fun ctx(): TxContext { dummy() }

    /// Get `TreasuryCap` for the TEST token.
    public fun get_treasury_cap(ctx: &mut TxContext): TreasuryCap<TEST> {
        coin::create_treasury_cap_for_testing(ctx)
    }

    /// Return `TreasuryCap` (shares it for now).
    public fun return_treasury_cap(treasury_cap: TreasuryCap<TEST>) {
        sui::transfer::public_share_object(treasury_cap)
    }

    /// Get a policy for testing.
    public fun get_policy(ctx: &mut TxContext): (TokenPolicy<TEST>, TokenPolicyCap<TEST>) {
        closed_loop::new_policy_for_testing(ctx)
    }

    /// Gracefully unpack policy after the tests have been performed.
    public fun return_policy(policy: TokenPolicy<TEST>, cap: TokenPolicyCap<TEST>) {
        closed_loop::burn_policy_for_testing(policy, cap)
    }

    /// Mint a test token.
    public fun mint(amount: u64, ctx: &mut TxContext): Token<TEST> {
        closed_loop::mint_for_testing(amount, ctx)
    }

    /// Burn a test token.
    public fun burn(token: Token<TEST>) {
        closed_loop::burn_for_testing(token)
    }
}
