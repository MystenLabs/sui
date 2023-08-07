// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::fs;
use std::path::PathBuf;
use sui_framework::{BuiltInFramework, SystemPackage};
use sui_framework_snapshot::update_bytecode_snapshot_manifest;
use sui_protocol_config::ProtocolVersion;

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

fn main() {
    // Always generate snapshot for the latest version.
    let version = ProtocolVersion::MAX.as_u64();
    let mut files = vec![];
    for package in BuiltInFramework::iter_system_packages() {
        write_package_to_file(version, package);
        files.push(*package.id());
    }
    update_bytecode_snapshot_manifest(GIT_REVISION, version, files);
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
