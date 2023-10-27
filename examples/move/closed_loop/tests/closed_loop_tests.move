// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only, allow(unused_function)]
module closed_loop::closed_loop_tests {
    use std::option;
    use std::string;
    use sui::tx_context::{Self, TxContext};
    use closed_loop::closed_loop::{Self, TokenPolicy, TokenPolicyCap};

    struct TEST has drop {}

    struct Rule1 has drop {}
    struct Rule2 has drop {}

    #[test]
    fun test_confirm_request() {
        let ctx = &mut tx_context::dummy();
        let (policy, cap) = get_policy(ctx);

        closed_loop::allow(&mut policy, &cap, string::utf8(b"test"), ctx);

        let req = closed_loop::new_request(
            string::utf8(b"test"), 100, option::none(), option::none(), ctx
        );

        closed_loop::confirm_request(&mut policy, req, ctx);
        return_policy(policy, cap)
    }

    #[test, expected_failure(abort_code = 0x0::closed_loop::EUnknownAction)]
    fun test_confirm_request_unknown_action_fail() {
        let ctx = &mut tx_context::dummy();
        let (policy, cap) = get_policy(ctx);
        let req = closed_loop::new_request(
            string::utf8(b"test"), 100, option::none(), option::none(), ctx
        );

        closed_loop::confirm_request(&mut policy, req, ctx);
        return_policy(policy, cap)
    }

    #[test, expected_failure(abort_code = 0x0::closed_loop::ESizeMismatch)]
    fun test_confirm_request_size_mismatch_fail() {
        let ctx = &mut tx_context::dummy();
        let (policy, cap) = get_policy(ctx);

        closed_loop::add_rule_for_action(
            Rule1 {},
            &mut policy,
            &cap,
            string::utf8(b"test"),
            false,
            ctx
        );

        let req = closed_loop::new_request(
            string::utf8(b"test"), 100, option::none(), option::none(), ctx
        );

        closed_loop::add_approval(Rule1 {}, &mut req, ctx);
        closed_loop::add_approval(Rule2 {}, &mut req, ctx);

        closed_loop::confirm_request(&mut policy, req, ctx);
        return_policy(policy, cap)
    }

    public fun get_policy(ctx: &mut TxContext): (TokenPolicy<TEST>, TokenPolicyCap<TEST>) {
        closed_loop::new_policy_for_testing(ctx)
    }

    public fun return_policy(policy: TokenPolicy<TEST>, cap: TokenPolicyCap<TEST>) {
        closed_loop::burn_policy_for_testing(policy, cap)
    }
}
