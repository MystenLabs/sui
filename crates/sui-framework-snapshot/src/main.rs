// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use sui_framework::{BuiltInFramework, SystemPackage};
use sui_protocol_config::ProtocolVersion;
use sui_types::base_types::ObjectID;

fn main() {
    let (network, git_version) = parse_args();
    // Always generate snapshot for the latest version.
    let version = ProtocolVersion::MAX.as_u64();
    let mut files = vec![];
    for package in BuiltInFramework::iter_system_packages() {
        write_package_to_file(&network, version, package);
        files.push(*package.id());
    }
    update_manifest(&network, git_version, version, files);
}

/// Parse args and return network name and git revision.
fn parse_args() -> (String, String) {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <devnet|testnet|mainnet> <git_version>", args[0]);
        std::process::exit(1);
    }

    // Check if the argument is one of the allowed values
    let allowed_values = ["devnet", "testnet", "mainnet"];
    let arg = args[1].as_str();
    if !allowed_values.contains(&arg) {
        eprintln!(
            "Error: argument must be one of {}",
            allowed_values.join(", ")
        );
        std::process::exit(1);
    }
    (args[1].clone(), args[2].clone())
}

fn write_package_to_file(network: &str, version: u64, package: &SystemPackage) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["bytecode_snapshot", network, version.to_string().as_str()]);
    fs::create_dir_all(&path)
        .or_else(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => Ok(()),
            _ => Err(e),
        })
        .expect("Unable to create snapshot directory");
    let bytes = bcs::to_bytes(package).expect("Deserialization cannot fail");
    fs::write(path.join(package.id().to_string()), bytes).expect("Unable to write data to file");
}

type SnapshotManifest = BTreeMap<String, BTreeMap<u64, SingleSnapshot>>;

#[derive(Serialize, Deserialize)]
struct SingleSnapshot {
    /// Git revision that this snapshot is taken on.
    git_revision: String,
    /// List of file names (also identical to object ID) of the bytecode package files.
    package_ids: Vec<ObjectID>,
}

fn update_manifest(network: &str, git_revision: String, version: u64, files: Vec<ObjectID>) {
    let filename = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bytecode_snapshot")
        .join("manifest.json");
    let mut snapshot = deserialize_manifest(&filename);

    let entry = snapshot
        .entry(network.to_owned())
        .or_insert_with(BTreeMap::new);
    entry.insert(
        version,
        SingleSnapshot {
            git_revision,
            package_ids: files,
        },
    );

    serialize_manifest(&filename, &snapshot);
}

fn deserialize_manifest(filename: &PathBuf) -> SnapshotManifest {
    let Ok(bytes) = fs::read(filename) else {
        return SnapshotManifest::default();
    };
    serde_json::from_slice::<SnapshotManifest>(&bytes)
        .expect("Could not deserialize SnapshotManifest")
}

fn serialize_manifest(filename: &PathBuf, snapshot: &SnapshotManifest) {
    let json =
        serde_json::to_string_pretty(snapshot).expect("Could not serialize SnapshotManifest");
    fs::write(filename, json).expect("");
}
