// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::package_config_tests;

use sui::package_config;
use sui::test_scenario as ts;

const SENDER: address = @42;
const PACKAGE_A: address = @100;
const PACKAGE_B: address = @101;
const CURRENT_VERSION: u64 = 5;

fun new_config(scenario: &mut ts::Scenario): package_config::PackageConfig {
    package_config::new_for_testing(scenario.ctx())
}

fun end(config: package_config::PackageConfig, scenario: ts::Scenario) {
    package_config::destroy_for_testing(config);
    scenario.end();
}

#[test]
fun test_forbid_and_allow_are_idempotent() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);

    config.forbid_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 1u64, scenario.ctx());
    config.forbid_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 1u64, scenario.ctx());
    assert!(config.is_version_forbidden_for_next_epoch(PACKAGE_A.to_id(), 1u64));

    config.allow_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 2u64, scenario.ctx());
    assert!(config.is_version_forbidden_for_next_epoch(PACKAGE_A.to_id(), 1u64));

    config.allow_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 1u64, scenario.ctx());
    config.allow_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 1u64, scenario.ctx());
    assert!(!config.is_version_forbidden_for_next_epoch(PACKAGE_A.to_id(), 1u64));

    end(config, scenario);
}

#[test]
fun test_forbid_list_is_per_package() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);

    config.forbid_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 2u64, scenario.ctx());
    assert!(config.is_version_forbidden_for_next_epoch(PACKAGE_A.to_id(), 2u64));
    assert!(!config.is_version_forbidden_for_next_epoch(PACKAGE_B.to_id(), 2u64));

    end(config, scenario);
}

#[test]
fun test_forbid_version_range_is_inclusive() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);

    config.forbid_version_range_for_testing(
        PACKAGE_A.to_id(),
        CURRENT_VERSION,
        1u64,
        4u64,
        scenario.ctx(),
    );
    assert!(config.is_version_forbidden_for_next_epoch(PACKAGE_A.to_id(), 1u64));
    assert!(config.is_version_forbidden_for_next_epoch(PACKAGE_A.to_id(), 2u64));
    assert!(config.is_version_forbidden_for_next_epoch(PACKAGE_A.to_id(), 3u64));
    assert!(config.is_version_forbidden_for_next_epoch(PACKAGE_A.to_id(), 4u64));

    end(config, scenario);
}

#[test, expected_failure(abort_code = sui::package_config::EInvalidVersion)]
fun test_forbid_zero_version_fails() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);
    config.forbid_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 0u64, scenario.ctx());
    end(config, scenario);
}

#[test, expected_failure(abort_code = sui::package_config::EInvalidVersion)]
fun test_forbid_current_version_fails() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);
    config.forbid_version_for_testing(
        PACKAGE_A.to_id(),
        CURRENT_VERSION,
        CURRENT_VERSION,
        scenario.ctx(),
    );
    end(config, scenario);
}

#[test, expected_failure(abort_code = sui::package_config::EInvalidVersion)]
fun test_allow_future_version_fails() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);
    config.allow_version_for_testing(
        PACKAGE_A.to_id(),
        CURRENT_VERSION,
        CURRENT_VERSION + 1u64,
        scenario.ctx(),
    );
    end(config, scenario);
}

#[test, expected_failure(abort_code = sui::package_config::EInvalidVersionRange)]
fun test_invalid_version_range_fails() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);
    config.forbid_version_range_for_testing(
        PACKAGE_A.to_id(),
        CURRENT_VERSION,
        4u64,
        1u64,
        scenario.ctx(),
    );
    end(config, scenario);
}
