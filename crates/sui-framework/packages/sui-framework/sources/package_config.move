// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// On-chain package configuration, keyed by original package ID.
///
/// The configuration hierarchy is:
/// ```
/// PackageConfig
///   └── PackageMetadataKey(original package ID)
///         └── Config<PackageConfigCap>
///               └── VersionForbiddenKey(package version number)
///                     └── Setting<u64> flags
/// ```
/// Each version has an independent `Setting` containg a flag. The flag word's high eight bits
/// (`63..56`) are a schema version, and its low 56 bits are schema-specific policy flags.
/// Schema version zero uses bit zero to mark the version forbidden. Readers treat unsupported
/// schema versions as forbidden, and mutation APIs reject them.
module sui::package_config;

use sui::config::{Self, Config};
use sui::dynamic_object_field as ofield;
use sui::package::{Self, UpgradeCap};

/// Trying to create the package config object when not called by the system address.
const ENotSystemAddress: u64 = 0;
/// The supplied version is not historical for the package controlled by the provided cap.
const EInvalidVersion: u64 = 1;
/// The start of a version range is greater than its end.
const EInvalidVersionRange: u64 = 2;
/// The forbid-list bitset schema version is not supported by this implementation.
const EUnsupportedBitsetVersion: u64 = 3;

/// The high eight bits identify the bitset schema version.
const BITSET_VERSION_SHIFT: u8 = 56;
/// Schema version zero's bit indicating that the package version is forbidden.
const VERSION_FORBIDDEN: u64 = 1;

/// A shared singleton that stores per-package configuration metadata.
public struct PackageConfig has key {
    id: UID,
}

/// The capability used to write to package configs. Ensures that per-package `Config`s are
/// modified only by this module.
public struct PackageConfigCap() has drop;

/// Dynamic object field key used to store a `Config` for an original package ID.
public struct PackageMetadataKey(ID) has copy, drop, store;

/// Setting key used to store forbid-list flags for one package version.
public struct VersionForbiddenKey(u64) has copy, drop, store;

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

fun is_supported_bitset_version(flags: u64): bool {
    (flags >> BITSET_VERSION_SHIFT) == 0
}

fun is_version_forbidden(flags: u64): bool {
    !is_supported_bitset_version(flags) || (flags & VERSION_FORBIDDEN) != 0
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
        |_package_config, _cap, _ctx| 0,
        |old_value, flags| {
            if (old_value.is_some()) *flags = old_value.destroy_some();
            assert!(is_supported_bitset_version(*flags), EUnsupportedBitsetVersion);
            *flags = *flags | VERSION_FORBIDDEN;
        },
        ctx,
    );
}

/// Forbid a historical version of the package controlled by `cap`.
public fun forbid_version(
    package_config: &mut PackageConfig,
    cap: &UpgradeCap,
    version: u64,
    ctx: &mut TxContext,
) {
    let current_version = package::version(cap);
    assert!(version > 0 && version < current_version, EInvalidVersion);
    forbid_version_impl(package_config, package::original_package_id(cap), version, ctx);
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
    start.range_do_eq!(end, |version| {
        package_config.forbid_version(cap, version, ctx);
    });
}

// Views and testing APIs

public(package) fun is_version_forbidden_for_next_epoch(
    package_config: &PackageConfig,
    original_id: ID,
    version: u64,
): bool {
    if (!package_config.per_package_metadata_exists(original_id)) return false;
    let config = package_config.borrow_per_package_config(original_id);
    let flags = config.read_setting_for_next_epoch<_, _, u64>(VersionForbiddenKey(version));
    if (flags.is_none()) return false;
    is_version_forbidden(flags.destroy_some())
}

// Test-only APIs for unit tests

#[test_only]
public(package) fun new_for_testing(ctx: &mut TxContext): PackageConfig {
    PackageConfig { id: object::new(ctx) }
}

#[test_only]
public(package) fun create_for_testing(ctx: &TxContext) {
    create(ctx);
}

#[test_only]
public(package) fun destroy_for_testing(package_config: PackageConfig) {
    let PackageConfig { id } = package_config;
    id.delete();
}

#[test_only]
public(package) fun set_version_flags_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    version: u64,
    flags: u64,
    ctx: &mut TxContext,
) {
    let config = package_config.per_package_config_entry!(original_id, ctx);
    config.update!(
        &mut PackageConfigCap(),
        VersionForbiddenKey(version),
        |_package_config, _cap, _ctx| flags,
        |_old_value, value| *value = flags,
        ctx,
    );
}

#[test_only]
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
        assert!(version > 0 && version < current_version, EInvalidVersion);
        package_config.forbid_version_impl(original_id, version, ctx);
    });
}

#[test_only]
public(package) fun forbid_version_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    current_version: u64,
    version: u64,
    ctx: &mut TxContext,
) {
    assert!(version > 0 && version < current_version, EInvalidVersion);
    forbid_version_impl(package_config, original_id, version, ctx);
}

#[test_only]
public(package) fun allow_version_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    current_version: u64,
    version: u64,
    ctx: &mut TxContext,
) {
    assert!(version > 0 && version < current_version, EInvalidVersion);
    if (package_config.per_package_metadata_exists(original_id)) {
        allow_version_impl(package_config, original_id, version, ctx);
    };
}

#[test_only]
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
        |_package_config, _cap, _ctx| 0,
        |old_value, flags| {
            if (old_value.is_some()) *flags = old_value.destroy_some();
            assert!(is_supported_bitset_version(*flags), EUnsupportedBitsetVersion);
            if ((*flags & VERSION_FORBIDDEN) != 0) *flags = *flags - VERSION_FORBIDDEN;
        },
        ctx,
    );
}
