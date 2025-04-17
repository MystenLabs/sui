// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An example of a Rule for the Closed Loop Token which limits the amount per
/// operation. Can be used to limit any action (eg transfer, toCoin, fromCoin).
module examples::limiter_rule {
    use std::string::String;
    use sui::{token::{Self, TokenPolicy, TokenPolicyCap, ActionRequest}, vec_map::{Self, VecMap}};

    /// Trying to perform an action that exceeds the limit.
    const ELimitExceeded: u64 = 0;

    /// The Rule witness.
    public struct Limiter has drop {}

    /// The Config object for the `lo
    public struct Config has store, drop {
        /// Mapping of Action -> Limit
        limits: VecMap<String, u64>,
    }

    /// Verifies that the request does not exceed the limit and adds an approval
    /// to the `ActionRequest`.
    public fun verify<T>(
        policy: &TokenPolicy<T>,
        request: &mut ActionRequest<T>,
        ctx: &mut TxContext,
    ) {
        if (!token::has_rule_config<T, Limiter>(policy)) {
            return token::add_approval(Limiter {}, request, ctx)
        };

        let config: &Config = token::rule_config(Limiter {}, policy);
        if (!vec_map::contains(&config.limits, &token::action(request))) {
            return token::add_approval(Limiter {}, request, ctx)
        };

        let action_limit = *vec_map::get(&config.limits, &token::action(request));

        assert!(token::amount(request) <= action_limit, ELimitExceeded);
        token::add_approval(Limiter {}, request, ctx);
    }

    /// Updates the config for the `Limiter` rule. Uses the `VecMap` to store
    /// the limits for each action.
    public fun set_config<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        limits: VecMap<String, u64>,
        ctx: &mut TxContext,
    ) {
        // if there's no stored config for the rule, add a new one
        if (!token::has_rule_config<T, Limiter>(policy)) {
            let config = Config { limits };
            token::add_rule_config(Limiter {}, policy, cap, config, ctx);
        } else {
            let config: &mut Config = token::rule_config_mut(Limiter {}, policy, cap);
            config.limits = limits;
        }
    }

    /// Returns the config for the `Limiter` rule.
    public fun get_config<T>(policy: &TokenPolicy<T>): VecMap<String, u64> {
        token::rule_config<T, Limiter, Config>(Limiter {}, policy).limits
    }
}

#[test_only]
module examples::limiter_rule_tests {
    use examples::limiter_rule::{Self as limiter, Limiter};
    use std::{option::none, string::utf8};
    use sui::{token, token_test_utils::{Self as test, TEST}, vec_map};

    #[test]
    // Scenario: add a limiter rule for 100 tokens per operation, verify that
    // the request with 100 tokens is confirmed
    fun add_limiter_default() {
        let ctx = &mut sui::tx_context::dummy();
        let (mut policy, cap) = test::get_policy(ctx);

        token::add_rule_for_action<TEST, Limiter>(&mut policy, &cap, utf8(b"action"), ctx);

        let mut request = token::new_request(utf8(b"action"), 100, none(), none(), ctx);

        limiter::verify(&policy, &mut request, ctx);

        token::confirm_request(&policy, request, ctx);
        test::return_policy(policy, cap);
    }

    #[test]
    // Scenario: add a limiter rule for 100 tokens per operation, verify that
    // the request with 100 tokens is confirmed; then remove the rule and verify
    // that the request with 100 tokens is not confirmed and repeat step (1)
    fun add_remove_limiter() {
        let ctx = &mut sui::tx_context::dummy();
        let (mut policy, cap) = test::get_policy(ctx);

        let mut config = vec_map::empty();
        vec_map::insert(&mut config, utf8(b"action"), 100);
        limiter::set_config(&mut policy, &cap, config, ctx);

        // adding limiter - confirmation required
        token::add_rule_for_action<TEST, Limiter>(&mut policy, &cap, utf8(b"action"), ctx);
        {
            let mut request = token::new_request(utf8(b"action"), 100, none(), none(), ctx);
            limiter::verify(&policy, &mut request, ctx);
            token::confirm_request(&policy, request, ctx);
        };

        // limiter removed - no confirmation required
        token::remove_rule_for_action<TEST, Limiter>(&mut policy, &cap, utf8(b"action"), ctx);
        {
            let request = token::new_request(utf8(b"action"), 100, none(), none(), ctx);
            token::confirm_request(&policy, request, ctx);
        };

        // limiter added but no limit now
        limiter::set_config(&mut policy, &cap, vec_map::empty(), ctx);
        token::add_rule_for_action<TEST, Limiter>(&mut policy, &cap, utf8(b"action"), ctx);
        {
            let mut request = token::new_request(utf8(b"action"), 100, none(), none(), ctx);
            limiter::verify(&policy, &mut request, ctx);
            token::confirm_request(&policy, request, ctx);
        };

        test::return_policy(policy, cap);
    }

    #[test, expected_failure(abort_code = examples::limiter_rule::ELimitExceeded)]
    // Scenario: add a limiter rule for 100 tokens per operation, verify that
    // the request with 101 tokens aborts with `ELimitExceeded`
    fun add_limiter_limit_exceeded_fail() {
        let ctx = &mut sui::tx_context::dummy();
        let (mut policy, cap) = test::get_policy(ctx);

        let mut config = vec_map::empty();
        vec_map::insert(&mut config, utf8(b"action"), 100);
        limiter::set_config(&mut policy, &cap, config, ctx);

        token::add_rule_for_action<TEST, Limiter>(&mut policy, &cap, utf8(b"action"), ctx);

        let mut request = token::new_request(utf8(b"action"), 101, none(), none(), ctx);
        limiter::verify(&policy, &mut request, ctx);

        abort 1337
    }
}
