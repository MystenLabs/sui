// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{fs, io::Read, path::PathBuf};
use sui_framework::{BuiltInFramework, SystemPackage};
use sui_protocol_config::ProtocolVersion;
use sui_types::base_types::ObjectID;

const GIT_REVISION: &str = {
    if let Some(revision) = option_env!("GIT_REVISION") {
        revision
    } else {
        let version = git_version::git_version!(
            args = ["--always", "--dirty", "--exclude", "*"],
            fallback = ""
        );

        if version.is_empty() {
            panic!("unable to query git revision");
        }
        version
    }
};

#[derive(Serialize, Deserialize)]
pub struct SingleSnapshot {
    /// Git revision that this snapshot is taken on.
    git_revision: String,
    /// List of file names (also identical to object ID) of the bytecode package files.
    package_ids: Vec<ObjectID>,
    /// Whether this bytecode version is already running in testnet or mainnet.
    /// This means that we cannot change it anymore.
    in_production: bool,
}

impl SingleSnapshot {
    pub fn git_revision(&self) -> &str {
        &self.git_revision
    }
    pub fn package_ids(&self) -> &[ObjectID] {
        &self.package_ids
    }

    pub fn set_in_production(&mut self) {
        self.in_production = true;
    }
}

pub struct SnapshotManifest {
    snapshots: BTreeMap<u64, SingleSnapshot>,
}

impl SnapshotManifest {
    pub fn new() -> Self {
        let bytes = fs::read(Self::manifest_path()).expect("Could not read manifest file");
        let snapshots = serde_json::from_slice::<BTreeMap<u64, SingleSnapshot>>(&bytes)
            .expect("Could not deserialize SnapshotManifest");
        Self { snapshots }
    }

    pub fn generate_new_snapshot(&mut self) {
        // Always generate snapshot for the latest version.
        let version = ProtocolVersion::MAX.as_u64();
        if let Some(snapshot) = self.snapshots.get(&version) {
            if snapshot.in_production {
                tracing::error!(
                    "Cannot update snapshot version {} that's already in production. Aborting",
                    version
                );
                return;
            }
            tracing::warn!(
                "Snapshot already exists for version {}. Overwriting",
                version
            );
        }
        let mut files = vec![];
        for package in BuiltInFramework::iter_system_packages() {
            write_package_to_file(version, package);
            files.push(*package.id());
        }
        self.update_bytecode_snapshot_manifest(GIT_REVISION, version, files);
        tracing::info!("Generated new bytecode snapshot for version {}", version)
    }

    fn update_bytecode_snapshot_manifest(
        &mut self,
        git_revision: &str,
        version: u64,
        files: Vec<ObjectID>,
    ) {
        self.snapshots.insert(
            version,
            SingleSnapshot {
                git_revision: git_revision.to_string(),
                package_ids: files,
                in_production: false,
            },
        );
    }

    pub fn release_latest_snapshot(&mut self) {
        let latest_version = *self.snapshots.keys().max().unwrap();
        let latest_snapshot = self.snapshots.get_mut(&latest_version).unwrap();
        if latest_snapshot.in_production {
            tracing::error!(
                "Snapshot version {} is already in production.",
                latest_version
            );
            return;
        }
        latest_snapshot.set_in_production();
    }

    fn save_bytecode_snapshot_manifest(&self) {
        let json = serde_json::to_string_pretty(&self.snapshots)
            .expect("Could not serialize SnapshotManifest");
        fs::write(Self::manifest_path(), json).expect("Could not update manifest file");
    }

    fn manifest_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("manifest.json")
    }
}

impl Drop for SnapshotManifest {
    fn drop(&mut self) {
        self.save_bytecode_snapshot_manifest();
    }
}

fn write_package_to_file(version: u64, package: &SystemPackage) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["bytecode_snapshot", version.to_string().as_str()]);
    fs::create_dir_all(&path)
        .or_else(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => Ok(()),
            _ => Err(e),
        })
        .expect("Unable to create snapshot directory");
    let bytes = bcs::to_bytes(package).expect("Deserialization cannot fail");
    fs::write(path.join(package.id().to_string()), bytes).expect("Unable to write data to file");
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

    // system packages need to be restored in a specific order.
    // This is needed when creating a genesis from a bytecode snapshot.
    let system_package_ids = sui_framework::BuiltInFramework::all_package_ids();
    assert!(snapshots.len() <= system_package_ids.len());
    let mut snapshot_objects = Vec::new();
    for package_id in system_package_ids {
        if let Some(object) = snapshots.remove(&package_id) {
            snapshot_objects.push(object);
        }
    }
    Ok(snapshot_objects)
}
