// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;

#[test]
fn bootstrap() {
    let root_dir = PathBuf::from(std::env!("CARGO_MANIFEST_DIR"));
    let proto_dir = root_dir.join("proto");
    let proto_ext = std::ffi::OsStr::new("proto");
    let proto_files = fs::read_dir(&proto_dir).and_then(|dir| {
        dir.filter_map(|entry| {
            (|| {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    return Ok(None);
                }

                let path = entry.path();
                if path.extension() != Some(proto_ext) {
                    return Ok(None);
                }

                Ok(Some(path))
            })()
            .transpose()
        })
        .collect::<Result<Vec<_>, _>>()
    });
    let proto_files = match proto_files {
        Ok(files) => files,
        Err(error) => panic!("failed to list proto files: {}", error),
    };

    let out_dir = root_dir.join("src").join("proto").join("generated");
    let file_descriptor_set_path = out_dir.join("sui.node.v2.fds.bin");

    if let Err(error) = tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .bytes(["."])
        .out_dir(&out_dir)
        .file_descriptor_set_path(file_descriptor_set_path)
        .compile_protos(&proto_files[..], &[proto_dir])
    {
        panic!("failed to compile `sui` protos: {}", error);
    }

    let status = std::process::Command::new("git")
        .arg("diff")
        .arg("--exit-code")
        .arg("--")
        .arg(out_dir)
        .status();
    match status {
        Ok(status) if !status.success() => panic!("You should commit the protobuf files"),
        Err(error) => panic!("failed to run `git diff`: {}", error),
        Ok(_) => {}
    }
}
