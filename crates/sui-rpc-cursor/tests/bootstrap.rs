// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::path::PathBuf;

use walkdir::WalkDir;

#[test]
fn bootstrap() {
    let root_dir = PathBuf::from(std::env!("CARGO_MANIFEST_DIR"));
    let proto_dir = root_dir.join("proto");
    let proto_ext = OsStr::new("proto");

    let mut proto_files = vec![];
    for entry in WalkDir::new(&proto_dir) {
        let entry = entry.unwrap();
        if entry.file_type().is_dir() {
            continue;
        }

        let path = entry.into_path();
        if path.extension() == Some(proto_ext) {
            proto_files.push(path)
        }
    }

    let out_dir = root_dir.join("src").join("proto").join("generated");

    let mut fds = protox::Compiler::new(std::slice::from_ref(&proto_dir))
        .unwrap()
        .include_source_info(true)
        .include_imports(true)
        .open_files(&proto_files)
        .unwrap()
        .file_descriptor_set();

    // Sort files by name to have deterministic codegen output
    fds.file.sort_by(|a, b| a.name.cmp(&b.name));

    // No gRPC services in this crate's protos, so client/server stub generation is off and no
    // FileDescriptorSet is emitted — these messages are only ever serialized as opaque cursor
    // bytes.
    if let Err(error) = tonic_prost_build::configure()
        .build_client(false)
        .build_server(false)
        .bytes(".")
        .out_dir(&out_dir)
        .compile_fds(fds)
    {
        panic!("failed to compile protos: {}", error);
    }

    let status = std::process::Command::new("git")
        .arg("diff")
        .arg("--exit-code")
        .arg("--")
        .arg(&out_dir)
        .status();
    match status {
        Ok(status) if !status.success() => panic!("You should commit the protobuf files"),
        Err(error) => panic!("failed to run `git diff`: {}", error),
        Ok(_) => {}
    }
}
