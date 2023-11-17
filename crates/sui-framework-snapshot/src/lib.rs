// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{fs, io::Read, path::PathBuf};
use sui_framework::SystemPackage;
use sui_types::base_types::ObjectID;
use sui_types::{
    BRIDGE_PACKAGE_ID, DEEPBOOK_PACKAGE_ID, MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID,
    SUI_SYSTEM_PACKAGE_ID,
};

pub type SnapshotManifest = BTreeMap<u64, SingleSnapshot>;

#[derive(Serialize, Deserialize)]
pub struct SingleSnapshot {
    /// Git revision that this snapshot is taken on.
    git_revision: String,
    /// List of file names (also identical to object ID) of the bytecode package files.
    package_ids: Vec<ObjectID>,
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

pub fn update_bytecode_snapshot_manifest(git_revision: &str, version: u64, files: Vec<ObjectID>) {
    let mut snapshot = load_bytecode_snapshot_manifest();

    snapshot.insert(
        version,
        SingleSnapshot {
            git_revision: git_revision.to_string(),
            package_ids: files,
        },
    );

    let json =
        serde_json::to_string_pretty(&snapshot).expect("Could not serialize SnapshotManifest");
    fs::write(manifest_path(), json).expect("Could not update manifest file");
}

pub fn load_bytecode_snapshot(protocol_version: u64) -> anyhow::Result<Vec<SystemPackage>> {
    let mut snapshot_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    snapshot_path.extend(["bytecode_snapshot", protocol_version.to_string().as_str()]);
    let mut snapshots: BTreeMap<ObjectID, SystemPackage> = fs::read_dir(&snapshot_path)?
        .flatten()
        .map(|entry| {
            let file_name = entry.file_name().to_str().unwrap().to_string();
            let mut file = fs::File::open(snapshot_path.clone().join(file_name))?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            let package: SystemPackage = bcs::from_bytes(&buffer)?;
            Ok((*package.id(), package))
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

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("manifest.json")
}
