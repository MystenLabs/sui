// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let sui_proto_dir = crate_dir.join("proto");
    let out_dir = crate_dir.join("src/proto/generated");

    println!("cargo:rerun-if-changed={}", sui_proto_dir.display());

    fs::create_dir_all(&out_dir).expect("create proto out dir");

    // Find all .proto files using walkdir
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

    let mut fds = protox::Compiler::new(&[sui_proto_dir.clone()])
        .unwrap()
        .include_source_info(true)
        .include_imports(true)
        .open_files(&proto_files)
        .unwrap()
        .file_descriptor_set();

    // Sort files by name to have deterministic codegen output
    fds.file.sort_by(|a, b| a.name.cmp(&b.name));

    let config = tonic_build::configure();

    config
        .build_client(true)
        .build_server(true)
        .type_attribute(".", "#[non_exhaustive]")
        .out_dir(&out_dir)
        .compile_fds(fds)
        .expect("compile event_service.proto");
}
