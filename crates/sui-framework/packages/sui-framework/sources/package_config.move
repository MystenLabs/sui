// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// On-chain package configuration, keyed directly by original package ID.
///
/// ```text
/// PackageConfig
/// ├── MinVersionKey { original_id }
/// │   └── MinVersion { version, package_id }
/// └── VersionForbiddenKey { original_id, version }
///     └── VERSION_FORBIDDEN
/// ```
module sui::package_config;

use sui::dynamic_field as field;
use sui::package::{Self, MinVersionEnrollment, MinVersionUpgrade, UpgradeCap};

/// A shared singleton that stores package policy dynamic fields.
public struct PackageConfig has key {
    id: UID,
}

/// Dynamic field key used to store the forbid-list value for one package version.
public struct VersionForbiddenKey has copy, drop, store {
    original_id: ID,
    version: u64,
}

/// Dynamic field key used to store the stable minversion selection for a package family.
public struct MinVersionKey has copy, drop, store {
    original_id: ID,
}

/// The package version selected by minversion.
public struct MinVersion has copy, drop, store {
    version: u64,
    package_id: ID,
}

const ENotSystemAddress: u64 = 0;
const EInvalidVersion: u64 = 1;
const EInvalidVersionRange: u64 = 2;
const VERSION_FORBIDDEN: u64 = 1;

/// Forbid a historical version of the package controlled by `cap`.
public fun forbid_version(
    package_config: &mut PackageConfig,
    cap: &mut UpgradeCap,
    version: u64,
    _ctx: &mut TxContext,
) {
    let (original_id, current_version) = cap_package_info(cap);
    assert_historical_version(version, current_version);
    forbid_version_impl(package_config, original_id, version);
}

/// Forbid all historical versions in the inclusive range `[start, end]`.
public fun forbid_version_range(
    package_config: &mut PackageConfig,
    cap: &mut UpgradeCap,
    start: u64,
    end: u64,
    _ctx: &mut TxContext,
) {
    assert!(start <= end, EInvalidVersionRange);
    let (original_id, current_version) = cap_package_info(cap);
    // `start <= end` and a historical end establish only the upper bound.
    assert!(start > 0, EInvalidVersion);
    assert_historical_version(end, current_version);
    package_config.forbid_version_range_impl(original_id, start, end);
}

public(package) fun is_version_forbidden(
    package_config: &PackageConfig,
    original_id: ID,
    version: u64,
): bool {
    let forbid_key = VersionForbiddenKey { original_id, version };
    field::get_fold!(&package_config.id, forbid_key, false, |value: &u64| is_forbidden_value(*value))
}

public fun record_minversion_enrollment(
    package_config: &mut PackageConfig,
    enrollment: MinVersionEnrollment,
    _ctx: &mut TxContext,
) {
    let (original_id, version, package_id) = package::minversion_enrollment_info(enrollment);
    package_config.record_minversion_impl(original_id, version, package_id);
}

public fun record_minversion_upgrade(
    package_config: &mut PackageConfig,
    upgrade: MinVersionUpgrade,
    _ctx: &mut TxContext,
) {
    let (original_id, _previous_version, version, package_id) =
        package::minversion_upgrade_info(upgrade);
    package_config.record_minversion_impl(original_id, version, package_id);
}

public fun record_minversion_upgrade_and_forbid_previous_versions(
    package_config: &mut PackageConfig,
    upgrade: MinVersionUpgrade,
    _ctx: &mut TxContext,
) {
    let (original_id, previous_version, version, package_id) =
        package::minversion_upgrade_info(upgrade);
    package_config.record_minversion_impl(original_id, version, package_id);
    package_config.forbid_version_range_impl(original_id, 1, previous_version);
}

#[allow(unused_function)]
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    transfer::share_object(PackageConfig {
        id: object::sui_package_config_object_id(),
    });
}

fun is_forbidden_value(value: u64): bool {
    value == VERSION_FORBIDDEN
}

fun record_minversion_impl(
    package_config: &mut PackageConfig,
    original_id: ID,
    version: u64,
    package_id: ID,
) {
    let key = MinVersionKey { original_id };
    let value = MinVersion { version, package_id };
    let _ = field::replace<MinVersionKey, MinVersion, MinVersion>(&mut package_config.id, key, value);
}

fun cap_package_info(cap: &UpgradeCap): (ID, u64) {
    (cap.original_package_id(), package::version(cap))
}

fun assert_historical_version(version: u64, current_version: u64) {
    assert!(version > 0 && version < current_version, EInvalidVersion);
}

fun forbid_version_impl(package_config: &mut PackageConfig, original_id: ID, version: u64) {
    let key = VersionForbiddenKey { original_id, version };
    let _ = field::replace<VersionForbiddenKey, u64, u64>(
        &mut package_config.id,
        key,
        VERSION_FORBIDDEN,
    );
}

fun forbid_version_range_impl(
    package_config: &mut PackageConfig,
    original_id: ID,
    start: u64,
    end: u64,
) {
    start.range_do_eq!(end, |version| package_config.forbid_version_impl(original_id, version));
}

#[mode(test)]
public(package) fun new_for_testing(ctx: &mut TxContext): PackageConfig {
    PackageConfig { id: object::new(ctx) }
}

#[mode(test)]
public(package) fun create_for_testing(ctx: &TxContext) {
    create(ctx);
}

#[mode(test)]
public(package) fun destroy_for_testing(package_config: PackageConfig) {
    let PackageConfig { id } = package_config;
    id.delete();
}

#[mode(test)]
public(package) fun minversion_version_for_testing(
    package_config: &PackageConfig,
    original_id: ID,
): Option<u64> {
    let key = MinVersionKey { original_id };
    if (!field::exists_with_type<_, MinVersion>(&package_config.id, key)) return option::none();
    let minversion: &MinVersion = field::borrow(&package_config.id, key);
    option::some(minversion.version)
}

#[mode(test)]
public(package) fun minversion_package_for_testing(
    package_config: &PackageConfig,
    original_id: ID,
): Option<ID> {
    let key = MinVersionKey { original_id };
    if (!field::exists_with_type<_, MinVersion>(&package_config.id, key)) return option::none();
    let minversion: &MinVersion = field::borrow(&package_config.id, key);
    option::some(minversion.package_id)
}

#[mode(test)]
public(package) fun forbid_version_range_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    current_version: u64,
    start: u64,
    end: u64,
    _ctx: &mut TxContext,
) {
    assert!(start <= end, EInvalidVersionRange);
    start.range_do_eq!(end, |version| {
        assert_historical_version(version, current_version);
        package_config.forbid_version_impl(original_id, version);
    });
}

#[mode(test)]
public(package) fun forbid_version_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    current_version: u64,
    version: u64,
    _ctx: &mut TxContext,
) {
    assert_historical_version(version, current_version);
    forbid_version_impl(package_config, original_id, version);
}

#[mode(test)]
public(package) fun allow_version_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    current_version: u64,
    version: u64,
    _ctx: &mut TxContext,
) {
    assert_historical_version(version, current_version);
    let key = VersionForbiddenKey { original_id, version };
    if (field::exists_with_type<_, u64>(&package_config.id, key)) {
        *field::borrow_mut(&mut package_config.id, key) = 0;
    }
}
