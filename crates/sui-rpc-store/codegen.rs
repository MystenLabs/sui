#!/usr/bin/env -S cargo +nightly -Zscript
---
[package]
name = "sui-rpc-store-proto-codegen"
edition = "2024"

[dependencies]
prost = "0.14.1"
prost-types = "0.14.1"
protox = "0.9"
tonic-prost-build = { version = "0.14.2", features = ["cleanup-markdown"] }
walkdir = "2.5.0"
proto-build = { git = "https://github.com/MystenLabs/sui-rust-sdk", branch = "master" }
---
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Single-file cargo script that regenerates the Rust code under
//! `crates/sui-rpc-store/src/proto/generated/` from the `.proto`
//! files under `crates/sui-rpc-store/proto/`. Mirrors
//! `sui-consistent-store/codegen.rs`; see that file for design
//! notes.
//!
//! Invoke from anywhere with:
//!
//! ```bash
//! cargo +nightly -Zscript path/to/codegen.rs
//! ```

use std::path::PathBuf;

use proto_build::codegen;
use proto_build::context;
use proto_build::message_graph::DescriptorGraph;

fn main() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let proto_dir = crate_dir.join("proto").canonicalize().expect(
        "expected a `proto/` subdirectory alongside this script; the script must \
         live in the crate root",
    );
    let out_dir = crate_dir.join("src/proto/generated");
    std::fs::create_dir_all(&out_dir).expect("failed to create out dir");

    let proto_ext = std::ffi::OsStr::new("proto");
    let mut proto_files: Vec<PathBuf> = walkdir::WalkDir::new(&proto_dir)
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().is_file() && entry.path().extension() == Some(proto_ext) {
                Some(entry.into_path())
            } else {
                None
            }
        })
        .collect();
    proto_files.sort();

    if proto_files.is_empty() {
        panic!("no .proto files found under {}", proto_dir.display());
    }

    let mut fds = protox::Compiler::new([&proto_dir])
        .expect("failed to construct protox compiler")
        .include_source_info(true)
        .include_imports(true)
        .open_files(&proto_files)
        .expect("failed to open proto files")
        .file_descriptor_set();
    fds.file.sort_by(|a, b| a.name.cmp(&b.name));

    if let Err(error) = tonic_prost_build::configure()
        .build_client(false)
        .build_server(false)
        .bytes(".")
        .btree_map(".")
        .generate_default_stubs(true)
        .out_dir(&out_dir)
        .compile_fds(fds.clone())
    {
        panic!("failed to compile protos: {error}");
    }

    let extern_paths = context::extern_paths::ExternPaths::new(&[], true)
        .expect("failed to build ExternPaths");
    let graph = DescriptorGraph::new(fds.file.iter());
    let ctx = context::Context::new(extern_paths, graph);
    codegen::accessors::generate_accessors(&ctx, &out_dir);
}
