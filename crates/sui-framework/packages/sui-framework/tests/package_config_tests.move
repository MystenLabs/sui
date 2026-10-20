// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::package_config_tests;

use sui::package;
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
fun test_minversion_enrollment_records_current_package() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);
    let mut cap = package::test_publish(PACKAGE_A.to_id(), scenario.ctx());

    let enrollment = cap.enable_minversion_for_testing(PACKAGE_A.to_id());
    config.record_minversion_enrollment(enrollment, scenario.ctx());

    assert!(cap.minversion_enabled());
    assert!(config
        .minversion_version_for_testing(PACKAGE_A.to_id())
        .destroy_some() == 1);
    assert!(config
        .minversion_package_for_testing(PACKAGE_A.to_id())
        .destroy_some() == PACKAGE_A.to_id());

    cap.make_immutable();
    end(config, scenario);
}

#[test]
fun test_restrict_preserves_enabled_minversion() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);
    let mut cap = package::test_publish(PACKAGE_A.to_id(), scenario.ctx());

    let enrollment = cap.enable_minversion_for_testing(PACKAGE_A.to_id());
    config.record_minversion_enrollment(enrollment, scenario.ctx());
    cap.only_additive_upgrades();
    assert!(cap.minversion_enabled());
    cap.only_dep_upgrades();
    assert!(cap.minversion_enabled());

    cap.make_immutable();
    end(config, scenario);
}

#[test]
fun test_minversion_upgrade_preserves_original_identity_across_authorization() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);
    let mut cap = package::test_publish(PACKAGE_A.to_id(), scenario.ctx());

    let enrollment = cap.enable_minversion_for_testing(PACKAGE_A.to_id());
    config.record_minversion_enrollment(enrollment, scenario.ctx());
    let authorization = cap.prepare_minversion_upgrade();
    let ticket = cap.authorize_upgrade(package::compatible_policy(), b"digest");
    let receipt = ticket.test_upgrade();
    let upgrade = cap.commit_minversion_upgrade(receipt, authorization);
    config.record_minversion_upgrade(upgrade, scenario.ctx());

    assert!(config
        .minversion_version_for_testing(PACKAGE_A.to_id())
        .destroy_some() == 2);
    cap.make_immutable();
    end(config, scenario);
}

#[test]
fun test_minversion_upgrade_can_forbid_previous_version() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);
    let mut cap = package::test_publish(PACKAGE_A.to_id(), scenario.ctx());

    let enrollment = cap.enable_minversion_for_testing(PACKAGE_A.to_id());
    config.record_minversion_enrollment(enrollment, scenario.ctx());

    let ticket = cap.authorize_upgrade(package::compatible_policy(), b"digest");
    let receipt = ticket.test_upgrade();
    let upgrade = cap.commit_minversion_upgrade_for_testing(receipt, PACKAGE_A.to_id());
    config.record_minversion_upgrade_and_forbid_previous(upgrade, scenario.ctx());

    assert!(config
        .minversion_version_for_testing(PACKAGE_A.to_id())
        .destroy_some() == 2);
    assert!(config.is_version_forbidden(PACKAGE_A.to_id(), 1));

    cap.make_immutable();
    end(config, scenario);
}

#[test, expected_failure(abort_code = sui::package::EMinVersionEnabled)]
fun test_minversion_cap_rejects_ordinary_upgrade_commit() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);
    let mut cap = package::test_publish(PACKAGE_A.to_id(), scenario.ctx());

    let enrollment = cap.enable_minversion_for_testing(PACKAGE_A.to_id());
    config.record_minversion_enrollment(enrollment, scenario.ctx());

    let ticket = cap.authorize_upgrade(package::compatible_policy(), b"digest");
    let receipt = ticket.test_upgrade();
    cap.commit_upgrade(receipt);
    abort
}

#[test, expected_failure(abort_code = sui::package::EMinVersionUnavailable)]
fun test_ordinary_cap_rejects_minversion_upgrade_commit() {
    let mut scenario = ts::begin(SENDER);
    let mut cap = package::test_publish(PACKAGE_A.to_id(), scenario.ctx());

    let ticket = cap.authorize_upgrade(package::compatible_policy(), b"digest");
    let receipt = ticket.test_upgrade();
    let _upgrade = cap.commit_minversion_upgrade_for_testing(receipt, PACKAGE_A.to_id());
    abort
}

#[test, expected_failure(abort_code = sui::package::EMinVersionUnavailable)]
fun test_permanently_disabled_cap_rejects_enrollment() {
    let mut scenario = ts::begin(SENDER);
    let mut cap = package::test_publish(PACKAGE_A.to_id(), scenario.ctx());

    cap.disable_minversion_permanently();
    assert!(cap.minversion_permanently_disabled());
    let _enrollment = cap.enable_minversion_for_testing(PACKAGE_A.to_id());
    abort
}

#[test]
fun test_forbid_and_allow_are_idempotent() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);

    config.forbid_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 1u64, scenario.ctx());
    config.forbid_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 1u64, scenario.ctx());
    assert!(config.is_version_forbidden(PACKAGE_A.to_id(), 1u64));

    config.allow_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 2u64, scenario.ctx());
    assert!(config.is_version_forbidden(PACKAGE_A.to_id(), 1u64));

    config.allow_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 1u64, scenario.ctx());
    config.allow_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 1u64, scenario.ctx());
    assert!(!config.is_version_forbidden(PACKAGE_A.to_id(), 1u64));

    end(config, scenario);
}

#[test]
fun test_forbid_list_is_per_package() {
    let mut scenario = ts::begin(SENDER);
    let mut config = new_config(&mut scenario);

    config.forbid_version_for_testing(PACKAGE_A.to_id(), CURRENT_VERSION, 2u64, scenario.ctx());
    assert!(config.is_version_forbidden(PACKAGE_A.to_id(), 2u64));
    assert!(!config.is_version_forbidden(PACKAGE_B.to_id(), 2u64));

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
    assert!(config.is_version_forbidden(PACKAGE_A.to_id(), 1u64));
    assert!(config.is_version_forbidden(PACKAGE_A.to_id(), 2u64));
    assert!(config.is_version_forbidden(PACKAGE_A.to_id(), 3u64));
    assert!(config.is_version_forbidden(PACKAGE_A.to_id(), 4u64));

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
