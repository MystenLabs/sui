// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// On-chain package configuration, keyed by original package ID.
///
/// The configuration hierarchy is:
/// ```
/// PackageConfig
///   └── PackageMetadataKey(original package ID)
///         └── Config<PackageConfigCap>
///               ├── VersionForbiddenKey(package version number)
///               │     └── Setting<u64> value
///               └── GlobalPauseKey()
///                     └── Setting<bool> value
/// ```
/// Each version has an independent `Setting`: `1` means the version is forbidden and `0` means it
/// is allowed. The global-pause setting applies to every version in the package family.
module sui::package_config;

use sui::config::{Self, Config};
use sui::dynamic_object_field as ofield;
use sui::package::{Self, UpgradeCap};

/// A shared singleton that stores per-package configuration metadata.
public struct PackageConfig has key {
    id: UID,
}

/// The capability used to write to package configs. Ensures that per-package `Config`s are
/// modified only by this module.
public struct PackageConfigCap() has drop;

/// Dynamic object field key used to store a `Config` for an original package ID.
public struct PackageMetadataKey(ID) has copy, drop, store;

/// Setting key used to store the forbid-list value for one package version.
public struct VersionForbiddenKey(u64) has copy, drop, store;

/// Setting key used to store the global-pause value for a package family.
public struct GlobalPauseKey() has copy, drop, store;

/// Trying to create the package config object when not called by the system address.
const ENotSystemAddress: u64 = 0;
/// The supplied version is not historical for the package controlled by the provided cap.
const EInvalidVersion: u64 = 1;
/// The start of a version range is greater than its end.
const EInvalidVersionRange: u64 = 2;
/// Value indicating that a package version is forbidden.
const VERSION_FORBIDDEN: u64 = 1;

/// Forbid a historical version of the package controlled by `cap`.
public fun forbid_version(
    package_config: &mut PackageConfig,
    cap: &UpgradeCap,
    version: u64,
    ctx: &mut TxContext,
) {
    let (original_id, current_version) = cap_package_info(cap);
    assert_historical_version(version, current_version);
    forbid_version_impl(package_config, original_id, version, ctx);
}

/// Forbid all historical versions in the inclusive range `[start, end]`.
public fun forbid_version_range(
    package_config: &mut PackageConfig,
    cap: &UpgradeCap,
    start: u64,
    end: u64,
    ctx: &mut TxContext,
) {
    assert!(start <= end, EInvalidVersionRange);
    let (original_id, current_version) = cap_package_info(cap);
    // Only need to check the end of the range since start <= end checked above.
    assert_historical_version(end, current_version);
    start.range_do_eq!(end, |version| {
        package_config.forbid_version_impl(original_id, version, ctx);
    });
}

public(package) fun is_version_forbidden_for_next_epoch(
    package_config: &PackageConfig,
    original_id: ID,
    version: u64,
): bool {
    if (!package_config.per_package_metadata_exists(original_id)) return false;
    let config = package_config.borrow_per_package_config(original_id);
    let value = config.read_setting_for_next_epoch<_, _, u64>(VersionForbiddenKey(version));
    if (value.is_none()) return false;
    is_version_forbidden(value.destroy_some())
}

/// Enable the global pause for every version of the package controlled by `cap`.
public fun enable_global_pause(
    package_config: &mut PackageConfig,
    cap: &UpgradeCap,
    ctx: &mut TxContext,
) {
    package_config.enable_global_pause_impl(cap.original_package_id(), ctx);
}

/// Disable the global pause for every version of the package controlled by `cap`.
///
/// If the setting exists, it is retained with a value of `false`.
public fun disable_global_pause(
    package_config: &mut PackageConfig,
    cap: &UpgradeCap,
    ctx: &mut TxContext,
) {
    package_config.disable_global_pause_impl(cap.original_package_id(), ctx);
}

public(package) fun is_global_pause_enabled_for_next_epoch(
    package_config: &PackageConfig,
    original_id: ID,
): bool {
    if (!package_config.per_package_metadata_exists(original_id)) return false;
    let config = package_config.borrow_per_package_config(original_id);
    config
        .read_setting_for_next_epoch<_, _, bool>(GlobalPauseKey())
        .destroy_or!(false)
}

#[allow(unused_function)]
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    transfer::share_object(PackageConfig {
        id: object::sui_package_config_object_id(),
    });
}

fun add_per_package_config(
    package_config: &mut PackageConfig,
    original_id: ID,
    ctx: &mut TxContext,
) {
    let key = PackageMetadataKey(original_id);
    let config = config::new(&mut PackageConfigCap(), ctx);
    ofield::internal_add(&mut package_config.id, key, config);
}

fun borrow_per_package_config_mut(
    package_config: &mut PackageConfig,
    original_id: ID,
): &mut Config<PackageConfigCap> {
    let key = PackageMetadataKey(original_id);
    ofield::internal_borrow_mut(&mut package_config.id, key)
}

fun borrow_per_package_config(
    package_config: &PackageConfig,
    original_id: ID,
): &Config<PackageConfigCap> {
    let key = PackageMetadataKey(original_id);
    ofield::internal_borrow(&package_config.id, key)
}

fun per_package_metadata_exists(package_config: &PackageConfig, original_id: ID): bool {
    let key = PackageMetadataKey(original_id);
    ofield::exists(&package_config.id, key)
}

fun is_version_forbidden(value: u64): bool {
    value == VERSION_FORBIDDEN
}

fun enable_global_pause_impl(
    package_config: &mut PackageConfig,
    original_id: ID,
    ctx: &mut TxContext,
) {
    let config = package_config.per_package_config_entry!(original_id, ctx);
    let next_epoch_entry = config.entry!<_, GlobalPauseKey, bool>(
        &mut PackageConfigCap(),
        GlobalPauseKey(),
        |_package_config, _cap, _ctx| true,
        ctx,
    );
    *next_epoch_entry = true;
}

fun disable_global_pause_impl(
    package_config: &mut PackageConfig,
    original_id: ID,
    ctx: &mut TxContext,
) {
    if (!package_config.per_package_metadata_exists(original_id)) return;

    let config = package_config.borrow_per_package_config_mut(original_id);
    let setting_name = GlobalPauseKey();
    if (!config.exists_with_type<_, GlobalPauseKey, bool>(setting_name)) return;
    config.update!(
        &mut PackageConfigCap(),
        setting_name,
        |_package_config, _cap, _ctx| false,
        |_old_value, value| *value = false,
        ctx,
    );
}

fun cap_package_info(cap: &UpgradeCap): (ID, u64) {
    (cap.original_package_id(), package::version(cap))
}

fun assert_historical_version(version: u64, current_version: u64) {
    assert!(version > 0 && version < current_version, EInvalidVersion);
}

fun forbid_version_impl(
    package_config: &mut PackageConfig,
    original_id: ID,
    version: u64,
    ctx: &mut TxContext,
) {
    let config = package_config.per_package_config_entry!(original_id, ctx);
    config.update!(
        &mut PackageConfigCap(),
        VersionForbiddenKey(version),
        |_package_config, _cap, _ctx| VERSION_FORBIDDEN,
        |_old_value, value| *value = VERSION_FORBIDDEN,
        ctx,
    );
}

macro fun per_package_config_entry(
    $package_config: &mut PackageConfig,
    $original_id: ID,
    $ctx: &mut TxContext,
): &mut Config<PackageConfigCap> {
    let package_config = $package_config;
    let original_id = $original_id;
    let ctx = $ctx;
    if (!package_config.per_package_metadata_exists(original_id)) {
        package_config.add_per_package_config(original_id, ctx);
    };
    package_config.borrow_per_package_config_mut(original_id)
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
public(package) fun global_pause_setting_exists_for_testing(
    package_config: &PackageConfig,
    original_id: ID,
): bool {
    if (!package_config.per_package_metadata_exists(original_id)) return false;
    package_config
        .borrow_per_package_config(original_id)
        .exists_with_type<_, GlobalPauseKey, bool>(GlobalPauseKey())
}

#[mode(test)]
public(package) fun enable_global_pause_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    ctx: &mut TxContext,
) {
    package_config.enable_global_pause_impl(original_id, ctx);
}

#[mode(test)]
public(package) fun disable_global_pause_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    ctx: &mut TxContext,
) {
    package_config.disable_global_pause_impl(original_id, ctx);
}

#[mode(test)]
public(package) fun forbid_version_range_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    current_version: u64,
    start: u64,
    end: u64,
    ctx: &mut TxContext,
) {
    assert!(start <= end, EInvalidVersionRange);
    start.range_do_eq!(end, |version| {
        assert_historical_version(version, current_version);
        package_config.forbid_version_impl(original_id, version, ctx);
    });
}

#[mode(test)]
public(package) fun forbid_version_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    current_version: u64,
    version: u64,
    ctx: &mut TxContext,
) {
    assert_historical_version(version, current_version);
    forbid_version_impl(package_config, original_id, version, ctx);
}

#[mode(test)]
public(package) fun allow_version_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    current_version: u64,
    version: u64,
    ctx: &mut TxContext,
) {
    assert_historical_version(version, current_version);
    if (package_config.per_package_metadata_exists(original_id)) {
        allow_version_impl(package_config, original_id, version, ctx);
    };
}

#[mode(test)]
fun allow_version_impl(
    package_config: &mut PackageConfig,
    original_id: ID,
    version: u64,
    ctx: &mut TxContext,
) {
    let config = package_config.borrow_per_package_config_mut(original_id);
    let setting_name = VersionForbiddenKey(version);
    if (!config.exists_with_type<_, VersionForbiddenKey, u64>(setting_name)) return;
    config.update!(
        &mut PackageConfigCap(),
        setting_name,
        |_package_config, _cap, _ctx| 0u64,
        |_old_value, value| *value = 0,
        ctx,
    );
}
