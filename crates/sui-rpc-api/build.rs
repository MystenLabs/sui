// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

fn find_dependency_proto_dir(package_name: &str, subpath: &str) -> PathBuf {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let crate_path = PathBuf::from(&crate_dir);
    let workspace_root = crate_path.parent().unwrap().parent().unwrap();

    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("metadata")
        .arg("--format-version=1")
        .current_dir(workspace_root);
    let output = cmd.output().expect("Failed to run cargo metadata");
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse cargo metadata");

    let package = metadata["packages"]
        .as_array()
        .expect("packages is not an array")
        .iter()
        .find(|pkg| pkg["name"] == package_name)
        .unwrap_or_else(|| panic!("{} package not found", package_name));

    let manifest_path = package["manifest_path"]
        .as_str()
        .expect("manifest_path not found");

    PathBuf::from(manifest_path).parent().unwrap().join(subpath)
}

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let sui_proto_dir = crate_dir.join("proto");
    let out_dir = crate_dir.join("src/proto/generated");

    let sui_rpc_proto_dir = find_dependency_proto_dir("sui-rpc", "vendored/proto");

    println!("cargo:rerun-if-changed={}", sui_proto_dir.display());
    println!("cargo:rerun-if-changed={}", sui_rpc_proto_dir.display());

    fs::create_dir_all(&out_dir).expect("create proto out dir");

    // Find all .proto files using walkdir (only from sui_proto_dir, not sui_rpc_proto_dir)
    let proto_ext = OsStr::new("proto");
    let mut proto_files = vec![];
    for entry in WalkDir::new(&sui_proto_dir) {
        let entry = entry.unwrap();
        if entry.file_type().is_dir() {
            continue;
        }

        let path = entry.into_path();
        if path.extension() == Some(proto_ext) {
            proto_files.push(path)
        }
    }

    let file_descriptors = protox::compile(proto_files, [sui_proto_dir, sui_rpc_proto_dir])
        .expect("failed to compile proto files");

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .type_attribute(".", "#[non_exhaustive]")
        .extern_path(".sui.rpc.v2", "::sui_rpc::proto::sui::rpc::v2")
        .out_dir(&out_dir)
        .compile_fds(file_descriptors)
        .expect("compile event_service.proto");
}
