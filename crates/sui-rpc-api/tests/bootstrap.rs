// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FileDescriptorSet;
use protox::prost::Message as _;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn bootstrap() {
    let root_dir = PathBuf::from(std::env!("CARGO_MANIFEST_DIR"));
    let proto_dir = root_dir.join("proto");
    let proto_ext = std::ffi::OsStr::new("proto");
    let proto_files = walkdir::WalkDir::new(&proto_dir)
        .into_iter()
        .filter_map(|entry| {
            (|| {
                let entry = entry?;
                if entry.file_type().is_dir() {
                    return Ok(None);
                }

                let path = entry.into_path();
                if path.extension() != Some(proto_ext) {
                    return Ok(None);
                }

                Ok(Some(path))
            })()
            .transpose()
        })
        .collect::<Result<Vec<_>, walkdir::Error>>()
        .unwrap();

    let out_dir = root_dir.join("src").join("proto").join("generated");

    let mut fds = protox::Compiler::new(&[proto_dir.clone()])
        .unwrap()
        .include_source_info(true)
        .include_imports(true)
        .open_files(&proto_files)
        .unwrap()
        .file_descriptor_set();
    // Sort files by name to have deterministic codegen output
    fds.file.sort_by(|a, b| a.name.cmp(&b.name));

    if let Err(error) = tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .bytes(["."])
        .btree_map([".sui.node.v2alpha.GetProtocolConfigResponse"])
        .out_dir(&out_dir)
        .compile_fds(fds.clone())
    {
        panic!("failed to compile protos: {}", error);
    }

    // Group the files by their package, in order to have a single fds file per package, and have
    // the files in the package sorted by their filename in order have a stable serialized format.
    let mut packages: HashMap<_, FileDescriptorSet> = HashMap::new();
    for mut file in fds.file {
        // Clear out the source code info as its not required for reflection
        file.source_code_info = None;
        packages
            .entry(file.package().to_owned())
            .or_default()
            .file
            .push(file);
    }

    for (package, fds) in packages {
        let file_name = format!("{package}.fds.bin");
        let file_descriptor_set_path = out_dir.join(&file_name);
        std::fs::write(file_descriptor_set_path, fds.encode_to_vec()).unwrap();
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
