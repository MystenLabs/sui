// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta::assert_snapshot;
use std::fs::write;
use std::path::PathBuf;
use sui_graphql_rpc::server::builder::export_schema;

#[test]
fn test_schema_sdl_export() {
    let sdl = export_schema();

    // update the current schema file
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["schema", "current_progress_schema.graphql"]);
    write(path, &sdl).unwrap();

    assert_snapshot!(sdl);
}
