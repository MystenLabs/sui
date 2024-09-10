// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// The goal of this module is to test Rule configuration setting and how Rules
/// can read / modify the configuration in 'em.
module sui::token_config_tests {
    use sui::token_test_utils::{Self as test, TEST};
    use sui::token;

    /// Rule witness to store configuration for
    public struct Rule1 has drop {}

    /// Configuration for the Rule1.
    public struct Config1 has store, drop { value: u64 }

    #[test]
    /// Scenario: create a Config, read it, mutate it, check existence and remove
    fun test_create_and_use_rule_config() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);
        let config = Config1 { value: 0 };

        // add a rule config
        token::add_rule_config(Rule1 {}, &mut policy, &cap, config, ctx);

        let config_mut: &mut Config1 = token::rule_config_mut(Rule1 {}, &mut policy, &cap);
        config_mut.value = 1000;

        // make sure rule can read config without Policy Owner
        let config_ref: &Config1 = token::rule_config(Rule1 {}, &policy);
        assert!(config_ref.value == 1000);
        assert!(token::has_rule_config<TEST, Rule1>(&policy));
        assert!(token::has_rule_config_with_type<TEST, Rule1, Config1>(&policy));

        let config = token::remove_rule_config<TEST, Rule1, Config1>(&mut policy, &cap, ctx);
        assert!(config.value == 1000);

        test::return_policy(policy, cap);
    }

    #[test, expected_failure(abort_code = token::ENotAuthorized)]
    /// Scenario: try to add config while not being authorized
    fun test_add_config_not_authorized_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, _cap) = test::get_policy(ctx);
        let (_policy, cap) = test::get_policy(ctx);
        let config = Config1 { value: 0 };

        token::add_rule_config(Rule1 {}, &mut policy, &cap, config, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = token::ENotAuthorized)]
    /// Scenario: try to add config while not being authorized
    fun test_remove_config_not_authorized_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);
        let (_policy, wrong_cap) = test::get_policy(ctx);
        let config = Config1 { value: 0 };

        token::add_rule_config(Rule1 {}, &mut policy, &cap, config, ctx);
        token::remove_rule_config<TEST, Rule1, Config1>(&mut policy, &wrong_cap, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = token::ENotAuthorized)]
    /// Scenario: try to mutate config while not being authorized
    fun test_mutate_config_not_authorized_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);
        let (_policy, wrong_cap) = test::get_policy(ctx);
        let config = Config1 { value: 0 };

        token::add_rule_config(Rule1 {}, &mut policy, &cap, config, ctx);
        token::rule_config_mut<TEST, Rule1, Config1>(Rule1 {}, &mut policy, &wrong_cap);

        abort 1337
    }

    #[test, expected_failure(abort_code = token::ENoConfig)]
    /// Scenario: rule tries to access a missing config
    fun test_rule_config_missing_config_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (policy, _cap) = test::get_policy(ctx);

        token::rule_config<TEST, Rule1, Config1>(Rule1 {}, &policy);

        abort 1337
    }

    #[test, expected_failure(abort_code = token::ENoConfig)]
    /// Scenario: rule tries to access a missing config
    fun test_rule_config_mut_missing_config_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);

        token::rule_config_mut<TEST, Rule1, Config1>(Rule1 {}, &mut policy, &cap);

        abort 1337
    }

    #[test, expected_failure(abort_code = token::ENoConfig)]
    /// Scenario: trying to remove a non existing config
    fun test_remove_rule_config_missing_config_fail() {
        let ctx = &mut test::ctx(@0x0);
        let (mut policy, cap) = test::get_policy(ctx);

        token::remove_rule_config<TEST, Rule1, Config1>(&mut policy, &cap, ctx);

        abort 1337
    }
}
