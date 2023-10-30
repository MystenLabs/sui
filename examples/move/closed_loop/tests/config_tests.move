// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// The goal of this module is to test Rule configuration setting and how Rules
/// can read / modify the configuration in 'em.
module closed_loop::config_tests {
    use closed_loop::test_utils::{Self as test, TEST};
    use closed_loop::closed_loop as cl;

    /// Rule witness to store confuration for
    struct Rule1 has drop {}

    /// Configuration for the Rule1.
    struct Config1 has store, drop { value: u64 }

    #[test]
    /// Scenario: create a Config, read it, mutate it, check existence and remove
    fun test_create_and_use_rule_config() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);
        let config = Config1 { value: 0 };

        // add a rule config
        cl::add_rule_config(Rule1 {}, &mut policy, &cap, config, ctx);

        let config_mut: &mut Config1 = cl::rule_config_mut(Rule1 {}, &mut policy, &cap);
        config_mut.value = 1000;

        // make sure rule can read config without Policy Owner
        let config_ref: &Config1 = cl::rule_config(Rule1 {}, &policy);
        assert!(config_ref.value == 1000, 0);
        assert!(cl::has_rule_config<TEST, Rule1>(&policy), 1);
        assert!(cl::has_rule_config_with_type<TEST, Rule1, Config1>(&policy), 2);

        let config = cl::remove_rule_config<TEST, Rule1, Config1>(&mut policy, &cap, ctx);
        assert!(config.value == 1000, 3);

        test::return_policy(policy, cap);
    }

    #[test, expected_failure(abort_code = cl::ENotAuthorized)]
    /// Scenario: try to add config while not being authorized
    fun test_add_config_fail_not_authorized() {
        let ctx = &mut test::ctx();
        let (policy, _cap) = test::get_policy(ctx);
        let (_policy, cap) = test::get_policy(ctx);
        let config = Config1 { value: 0 };

        cl::add_rule_config(Rule1 {}, &mut policy, &cap, config, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = cl::ENotAuthorized)]
    /// Scenario: try to add config while not being authorized
    fun test_remove_config_fail_not_authorized() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);
        let (_policy, wrong_cap) = test::get_policy(ctx);
        let config = Config1 { value: 0 };

        cl::add_rule_config(Rule1 {}, &mut policy, &cap, config, ctx);
        cl::remove_rule_config<TEST, Rule1, Config1>(&mut policy, &wrong_cap, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = cl::ENotAuthorized)]
    /// Scenario: try to mutate config while not being authorized
    fun test_mutate_config_fail_not_authorized() {
        let ctx = &mut test::ctx();
        let (policy, cap) = test::get_policy(ctx);
        let (_policy, wrong_cap) = test::get_policy(ctx);
        let config = Config1 { value: 0 };

        cl::add_rule_config(Rule1 {}, &mut policy, &cap, config, ctx);
        cl::rule_config_mut<TEST, Rule1, Config1>(Rule1 {}, &mut policy, &wrong_cap);

        abort 1337
    }
}
