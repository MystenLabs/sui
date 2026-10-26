// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// On-chain package configuration, keyed directly by original package ID.
///
/// ```text
/// PackageConfig
/// ├── MinVersionKey { original_id }
/// │   └── MinVersion { version, package_id }
/// ├── VersionsForbiddenThroughKey { original_id }
/// │   └── a version through which every version is known to be forbidden
/// └── VersionForbiddenKey { original_id, chunk }
///     └── a `u256` bitmap for 256 consecutive package versions
/// ```
module sui::package_config;

use sui::dynamic_field as field;
use sui::package::{Self, MinVersionEnrollment, UpgradeCap, UpgradeReceipt};

/// A shared singleton that stores package policy dynamic fields.
public struct PackageConfig has key {
    id: UID,
}

/// Dynamic field key used to store a bitmap of 256 forbidden package versions.
public struct VersionForbiddenKey has copy, drop, store {
    original_id: ID,
    chunk: u64,
}

/// Dynamic field key recording a version through which every version in a package family is
/// known to be forbidden.
public struct VersionsForbiddenThroughKey has copy, drop, store {
    original_id: ID,
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
const EInvalidMinVersionState: u64 = 3;
const ETooManyVersionsToForbid: u64 = 4;
const VERSIONS_PER_FORBID_CHUNK: u64 = 256;
const MAX_VERSIONS_PER_FORBID: u64 = 512;

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
    package_config.advance_versions_forbidden_through(
        original_id,
        version,
        version,
        MAX_VERSIONS_PER_FORBID - 1,
    );
}

/// Forbid at most 512 historical versions in the inclusive range `[start, end]`.
/// If the range connects to the tracked forbidden prefix, extend the prefix through `end`.
public fun forbid_version_range(
    package_config: &mut PackageConfig,
    cap: &mut UpgradeCap,
    start: u64,
    end: u64,
    _ctx: &mut TxContext,
) {
    assert_valid_forbid_range(start, end);
    let range_size = end - start + 1;
    let (original_id, current_version) = cap_package_info(cap);
    // A valid range and a historical end establish only the upper bound.
    assert!(start > 0, EInvalidVersion);
    assert_historical_version(end, current_version);
    package_config.forbid_version_range_impl(original_id, start, end);
    package_config.advance_versions_forbidden_through(
        original_id,
        start,
        end,
        MAX_VERSIONS_PER_FORBID - range_size,
    );
}

public(package) fun is_version_forbidden(
    package_config: &PackageConfig,
    original_id: ID,
    version: u64,
): bool {
    assert!(version > 0, EInvalidVersion);
    let (chunk, bit) = version_location(version);
    let bits = package_config.forbidden_chunk_bits(original_id, chunk);
    bits & bit_mask(bit) != 0
}

public fun record_minversion_enrollment(
    package_config: &mut PackageConfig,
    enrollment: MinVersionEnrollment,
    _ctx: &mut TxContext,
) {
    let (original_id, version, package_id) = package::minversion_enrollment_info(enrollment);
    let old = package_config.replace_minversion(original_id, version, package_id);
    assert!(old.is_none(), EInvalidMinVersionState);
    old.destroy_none();
}

public fun record_minversion_upgrade(
    package_config: &mut PackageConfig,
    cap: &UpgradeCap,
    receipt: &UpgradeReceipt,
    _ctx: &mut TxContext,
) {
    let (original_id, previous_version, previous_package_id, package_id) =
        package::record_minversion_upgrade_info(cap, receipt);
    package_config.record_minversion_upgrade_impl(
        original_id,
        previous_version,
        previous_package_id,
        package_id,
    );
}

public fun record_minversion_upgrade_and_forbid_previous_versions(
    package_config: &mut PackageConfig,
    cap: &UpgradeCap,
    receipt: &UpgradeReceipt,
    _ctx: &mut TxContext,
) {
    let (original_id, previous_version, previous_package_id, package_id) =
        package::record_minversion_upgrade_info(cap, receipt);
    package_config.record_minversion_upgrade_impl(
        original_id,
        previous_version,
        previous_package_id,
        package_id,
    );
    package_config.forbid_previous_versions_impl(original_id, previous_version);
}

#[allow(unused_function)]
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);
    transfer::share_object(PackageConfig {
        id: object::sui_package_config_object_id(),
    });
}

fun record_minversion_upgrade_impl(
    package_config: &mut PackageConfig,
    original_id: ID,
    previous_version: u64,
    previous_package_id: ID,
    package_id: ID,
) {
    let old = package_config.replace_minversion(original_id, previous_version + 1, package_id);
    assert!(old.is_some(), EInvalidMinVersionState);
    let MinVersion { version, package_id } = old.destroy_some();
    assert!(
        version == previous_version && package_id == previous_package_id,
        EInvalidMinVersionState,
    );
}

fun replace_minversion(
    package_config: &mut PackageConfig,
    original_id: ID,
    version: u64,
    package_id: ID,
): Option<MinVersion> {
    let key = MinVersionKey { original_id };
    let value = MinVersion { version, package_id };
    field::replace<MinVersionKey, MinVersion, MinVersion>(&mut package_config.id, key, value)
}

fun cap_package_info(cap: &UpgradeCap): (ID, u64) {
    (cap.original_package_id(), package::version(cap))
}

fun assert_historical_version(version: u64, current_version: u64) {
    assert!(version > 0 && version < current_version, EInvalidVersion);
}

fun assert_valid_forbid_range(start: u64, end: u64) {
    assert!(start <= end, EInvalidVersionRange);
    assert!(end - start < MAX_VERSIONS_PER_FORBID, ETooManyVersionsToForbid);
}

/// Returns the bitmap chunk and bit position for a nonzero package version.
/// For example, versions 1, 256, and 257 map to `(0, 0)`, `(0, 255)`, and `(1, 0)`.
fun version_location(version: u64): (u64, u8) {
    let index = version - 1;
    (
        index / VERSIONS_PER_FORBID_CHUNK,
        (index % VERSIONS_PER_FORBID_CHUNK) as u8,
    )
}

fun bit_mask(bit: u8): u256 {
    1u256 << bit
}

/// Returns a mask with bits zero through `bit` set, inclusive.
/// For example, `prefix_mask(3)` is `0b1111`.
fun prefix_mask(bit: u8): u256 {
    if (bit == 255) std::u256::max_value!() else (1u256 << (bit + 1)) - 1
}

/// Returns a mask with bits `first` through `last` set, inclusive.
/// For example, `range_mask(2, 4)` is `0b1_1100`.
fun range_mask(first: u8, last: u8): u256 {
    let before_first = if (first == 0) 0 else prefix_mask(first - 1);
    prefix_mask(last) ^ before_first
}

/// Returns the stored forbidden-version bitmap for `chunk`, or zero if it does not exist.
/// For example, `0b101` in chunk zero means package versions 1 and 3 are forbidden.
fun forbidden_chunk_bits(package_config: &PackageConfig, original_id: ID, chunk: u64): u256 {
    let key = VersionForbiddenKey { original_id, chunk };
    field::get_fold!(&package_config.id, key, 0u256, |bits: &u256| *bits)
}

/// Marks each bit in `mask` as forbidden in `chunk`, creating the chunk if it does not exist.
fun mark_chunk_bits_forbidden(
    package_config: &mut PackageConfig,
    original_id: ID,
    chunk: u64,
    mask: u256,
) {
    let key = VersionForbiddenKey { original_id, chunk };
    let bits = field::borrow_mut_or_add!(&mut package_config.id, key, 0u256);
    *bits = *bits | mask;
}

fun forbid_version_impl(package_config: &mut PackageConfig, original_id: ID, version: u64) {
    let (chunk, bit) = version_location(version);
    package_config.mark_chunk_bits_forbidden(original_id, chunk, bit_mask(bit));
}

fun forbid_version_range_impl(
    package_config: &mut PackageConfig,
    original_id: ID,
    start: u64,
    end: u64,
) {
    let (first_chunk, first_bit) = version_location(start);
    let (last_chunk, last_bit) = version_location(end);
    first_chunk.range_do_eq!(last_chunk, |chunk| {
        let first = if (chunk == first_chunk) first_bit else 0;
        let last = if (chunk == last_chunk) last_bit else 255;
        package_config.mark_chunk_bits_forbidden(original_id, chunk, range_mask(first, last));
    });
}

fun forbid_previous_versions_impl(
    package_config: &mut PackageConfig,
    original_id: ID,
    end: u64,
) {
    let forbidden_through = package_config.versions_forbidden_through(original_id);
    if (forbidden_through >= end) return;

    // `forbidden_through < end` guarantees that this addition cannot overflow.
    let start = forbidden_through + 1;
    assert_valid_forbid_range(start, end);
    let range_size = end - start + 1;
    package_config.forbid_version_range_impl(original_id, start, end);
    package_config.advance_versions_forbidden_through(
        original_id,
        start,
        end,
        MAX_VERSIONS_PER_FORBID - range_size,
    );
}

fun scan_forbidden_through(
    package_config: &PackageConfig,
    original_id: ID,
    mut forbidden_through: u64,
    mut remaining: u64,
): u64 {
    while (remaining > 0 && forbidden_through < std::u64::max_value!()) {
        let (chunk, mut bit) = version_location(forbidden_through + 1);
        let bits = package_config.forbidden_chunk_bits(original_id, chunk);
        while (remaining > 0) {
            remaining = remaining - 1;
            if (bits & bit_mask(bit) == 0) return forbidden_through;
            forbidden_through = forbidden_through + 1;
            if (bit == 255 || forbidden_through == std::u64::max_value!()) break;
            bit = bit + 1;
        };
    };
    forbidden_through
}

fun advance_versions_forbidden_through(
    package_config: &mut PackageConfig,
    original_id: ID,
    start: u64,
    end: u64,
    remaining: u64,
) {
    let old_forbidden_through = package_config.versions_forbidden_through(original_id);
    if (end > old_forbidden_through && start > old_forbidden_through + 1) return;

    let forbidden_through = if (end > old_forbidden_through) end else old_forbidden_through;
    let forbidden_through = package_config.scan_forbidden_through(
        original_id,
        forbidden_through,
        remaining,
    );
    if (forbidden_through == old_forbidden_through) return;

    let key = VersionsForbiddenThroughKey { original_id };
    field::replace<VersionsForbiddenThroughKey, u64, u64>(
        &mut package_config.id,
        key,
        forbidden_through,
    );
}

fun versions_forbidden_through(package_config: &PackageConfig, original_id: ID): u64 {
    let key = VersionsForbiddenThroughKey { original_id };
    field::get_fold!(&package_config.id, key, 0, |version: &u64| *version)
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
    assert_valid_forbid_range(start, end);
    let range_size = end - start + 1;
    assert!(start > 0, EInvalidVersion);
    assert_historical_version(end, current_version);
    package_config.forbid_version_range_impl(original_id, start, end);
    package_config.advance_versions_forbidden_through(
        original_id,
        start,
        end,
        MAX_VERSIONS_PER_FORBID - range_size,
    );
}

#[mode(test)]
public(package) fun forbid_previous_versions_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    end: u64,
) {
    package_config.forbid_previous_versions_impl(original_id, end);
}

#[mode(test)]
public(package) fun versions_forbidden_through_for_testing(
    package_config: &PackageConfig,
    original_id: ID,
): u64 {
    package_config.versions_forbidden_through(original_id)
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
    package_config.advance_versions_forbidden_through(
        original_id,
        version,
        version,
        MAX_VERSIONS_PER_FORBID - 1,
    );
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
    let (chunk, bit) = version_location(version);
    let key = VersionForbiddenKey { original_id, chunk };
    if (field::exists_with_type<_, u256>(&package_config.id, key)) {
        let bits: &mut u256 = field::borrow_mut(&mut package_config.id, key);
        *bits = *bits & std::u256::bitwise_not(bit_mask(bit));
    };
    if (package_config.versions_forbidden_through(original_id) >= version) {
        let key = VersionsForbiddenThroughKey { original_id };
        field::replace<VersionsForbiddenThroughKey, u64, u64>(
            &mut package_config.id,
            key,
            version - 1,
        );
    }
}

#[mode(test)]
public(package) fun forbidden_chunk_for_testing(
    package_config: &PackageConfig,
    original_id: ID,
    chunk: u64,
): u256 {
    package_config.forbidden_chunk_bits(original_id, chunk)
}

#[mode(test)]
public(package) fun set_minversion_for_testing(
    package_config: &mut PackageConfig,
    original_id: ID,
    version: u64,
    package_id: ID,
) {
    let _ = package_config.replace_minversion(original_id, version, package_id);
}
