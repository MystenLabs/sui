// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// This module defines base testing utilities for the
module sui::token_test_utils {
    use sui::coin::{Self, TreasuryCap};
    use sui::token::{Self, Token, TokenPolicy, TokenPolicyCap};

    /// The type of the test Token.
    public struct TEST has drop {}

    /// Get a context for testing.
    public fun ctx(sender: address): TxContext {
        let tx_hash = x"3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532";
        tx_context::new(sender, tx_hash, 0, 0, 0)
    }

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
        token::new_policy_for_testing(ctx)
    }

    /// Gracefully unpack policy after the tests have been performed.
    public fun return_policy(policy: TokenPolicy<TEST>, cap: TokenPolicyCap<TEST>) {
        policy.burn_policy_for_testing(cap)
    }

    /// Mint a test token.
    public fun mint(amount: u64, ctx: &mut TxContext): Token<TEST> {
        token::mint_for_testing(amount, ctx)
    }

    /// Burn a test token.
    public fun burn(token: Token<TEST>) {
        token.burn_for_testing()
    }
}
