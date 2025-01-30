// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context};
use sui_framework_snapshot::{SingleSnapshot, FRAMEWORK_MANIFEST};
use sui_protocol_config::ProtocolVersion;

/// Return the framework snapshot for the latest known protocol version
pub fn latest_framework() -> &'static SingleSnapshot {
    FRAMEWORK_MANIFEST.last_key_value().expect("").1
}

/// Return the best commit hash for the given protocol version. Gives an error if [version]
/// is newer than the maximum protocol version or older than the first known framework.
pub fn framework_for_protocol(version: ProtocolVersion) -> anyhow::Result<&'static SingleSnapshot> {
    if version > ProtocolVersion::MAX {
        bail!("Protocol version {version:?} is newer than this CLI.");
    }

    Ok(FRAMEWORK_MANIFEST
        .range(..=version.as_u64())
        .next_back()
        .context(format!("Unrecognized protocol version {version:?}"))?
        .1)
}

#[test]
/// the hash for a specific version that we have one for is corretly returned
fn test_hash_exact() {
    assert_eq!(
        framework_for_protocol(4.into()).unwrap().git_revision(),
        "f5d26f1b3ae89f68cb66f3a007e90065e5286905"
    );
}

#[test]
/// we get the right hash for a version that we don't have an exact entry for
fn test_hash_gap() {
    // versions 56 and 57 are missing in the manifest; version 55 should be returned
    assert_eq!(
        framework_for_protocol(56.into()).unwrap().git_revision(),
        framework_for_protocol(55.into()).unwrap().git_revision(),
    );
    assert_eq!(
        framework_for_protocol(57.into()).unwrap().git_revision(),
        framework_for_protocol(55.into()).unwrap().git_revision(),
    );
}

#[test]
/// we get the correct hash for the latest known protocol version
fn test_hash_latest() {
    assert_eq!(
        framework_for_protocol(ProtocolVersion::MAX)
            .unwrap()
            .git_revision(),
        latest_framework_snapshot().git_revision()
    );
}

#[test]
/// we get an error if the protocol version is too small or too large
fn test_hash_errors() {
    assert!(framework_for_protocol(0.into()).is_err());
    assert!(framework_for_protocol(ProtocolVersion::MAX + 1).is_err());
}
