// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::{fs, io::Read, path::PathBuf};
use sui_framework::{SystemPackage, SystemPackageMetadata};
use sui_types::base_types::ObjectID;
use sui_types::{
    BRIDGE_PACKAGE_ID, DEEPBOOK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID,
    SUI_SYSTEM_PACKAGE_ID,
};

pub type SnapshotManifest = BTreeMap<u64, Snapshot>;

/// Encapsulation of an entry in the manifest file corresponding to a single version of the system
/// packages.
///
// Note: the [Snapshot] and [SnapshotPackage] types are similar to the
// [sui_framework::{SystemPackageMetadata, SystemPackage}] types,
// and also to the [sui::framework_versions::{FrameworkVersion, FrameworkPackage}] types.
// They are sort of a stepping stone from one to the other - the [sui_framework] types contain
// additional information about the compiled bytecode of the package, while the
// [framework_versions] types do not contain information about the object IDs of the packages.
//
// These types serve as a kind of stepping stone; they are constructed from the [sui_framework]
// types and serialized in the manifest, and then the build script for the [sui] crate reads them
// from the manifest file and encodes them in the `sui` binary. A little information is dropped in
// each of these steps.
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    /// Git revision that this snapshot is taken on.
    pub git_revision: String,

    /// List of system packages in this version
    pub packages: Vec<SnapshotPackage>,
}

/// Entry in the manifest file corresponding to a specific version of a specific system package
#[derive(Serialize, Deserialize)]
pub struct SnapshotPackage {
    /// Name of the package (e.g. "MoveStdLib")
    pub name: String,
    /// Path to the package in the monorepo (e.g. "crates/sui-framework/packages/move-stdlib")
    pub path: String,
    /// Object ID of the published package
    pub id: ObjectID,
}

impl Snapshot {
    pub fn package_ids(&self) -> impl Iterator<Item = ObjectID> + '_ {
        self.packages.iter().map(|p| p.id)
    }
}

impl SnapshotPackage {
    pub fn from_system_package_metadata(value: &SystemPackageMetadata) -> Self {
        Self {
            name: value.name.clone(),
            path: value.path.clone(),
            id: value.compiled.id,
        }
    }
}

const SYSTEM_PACKAGE_PUBLISH_ORDER: &[ObjectID] = &[
    MOVE_STDLIB_PACKAGE_ID,
    SUI_FRAMEWORK_PACKAGE_ID,
    SUI_SYSTEM_PACKAGE_ID,
    DEEPBOOK_PACKAGE_ID,
    BRIDGE_PACKAGE_ID,
];

pub fn load_bytecode_snapshot_manifest() -> SnapshotManifest {
    let Ok(bytes) = fs::read(manifest_path()) else {
        return SnapshotManifest::default();
    };
    serde_json::from_slice::<SnapshotManifest>(&bytes)
        .expect("Could not deserialize SnapshotManifest")
}

pub fn update_bytecode_snapshot_manifest(
    git_revision: &str,
    version: u64,
    files: Vec<SnapshotPackage>,
) {
    let mut snapshot = load_bytecode_snapshot_manifest();

    snapshot.insert(
        version,
        Snapshot {
            git_revision: git_revision.to_string(),
            packages: files,
        },
    );

    let json =
        serde_json::to_string_pretty(&snapshot).expect("Could not serialize SnapshotManifest");
    fs::write(manifest_path(), json).expect("Could not update manifest file");
}

pub fn load_bytecode_snapshot(protocol_version: u64) -> anyhow::Result<Vec<SystemPackage>> {
    let snapshot_path = snapshot_path_for_version(protocol_version)?;
    let mut snapshots: BTreeMap<ObjectID, SystemPackage> = fs::read_dir(&snapshot_path)?
        .flatten()
        .map(|entry| {
            let file_name = entry.file_name().to_str().unwrap().to_string();
            let mut file = fs::File::open(snapshot_path.clone().join(file_name))?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            let package: SystemPackage = bcs::from_bytes(&buffer)?;
            Ok((package.id, package))
        })
        .collect::<anyhow::Result<_>>()?;

    // system packages need to be restored in a specific order
    assert!(snapshots.len() <= SYSTEM_PACKAGE_PUBLISH_ORDER.len());
    let mut snapshot_objects = Vec::new();
    for package_id in SYSTEM_PACKAGE_PUBLISH_ORDER {
        if let Some(object) = snapshots.remove(package_id) {
            snapshot_objects.push(object);
        }
    }
    Ok(snapshot_objects)
}

pub fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("manifest.json")
}

/// Given a protocol version:
/// * The path to the snapshot directory for that version is returned, if it exists.
/// * If the version is greater than the latest snapshot version, then `Ok(None)` is returned.
/// * If the version does not exist, but there are snapshots present with versions greater than
///   `version`, then the smallest snapshot number greater than `version` is returned.
fn snapshot_path_for_version(version: u64) -> anyhow::Result<PathBuf> {
    let snapshot_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bytecode_snapshot");
    let mut snapshots = BTreeSet::new();

    for entry in fs::read_dir(&snapshot_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(snapshot_number) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.parse::<u64>().ok())
            {
                snapshots.insert(snapshot_number);
            }
        }
    }

    snapshots
        .range(version..)
        .next()
        .map(|v| snapshot_dir.join(v.to_string()))
        .ok_or_else(|| anyhow::anyhow!("No snapshot found for version {}", version))
}
