// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use fs_extra::dir::CopyOptions;
use tempfile::TempDir;

#[cfg(test)]
mod metered_verifier;

/// Copy the examples folder and the sui-framework packages into a temporary directory and return
/// the temporary directory
pub fn setup_examples() -> TempDir {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
    let result = tempfile::tempdir().unwrap();

    fs_extra::dir::copy(
        repo_root.join("examples"),
        &result,
        &CopyOptions::new().content_only(false),
    )
    .unwrap();

    let framework_tmp = result.path().join("crates/sui-framework/");
    std::fs::create_dir_all(&framework_tmp).unwrap();
    fs_extra::dir::copy(
        repo_root.join("crates/sui-framework/packages"),
        &framework_tmp,
        &CopyOptions::new().content_only(false),
    )
    .unwrap();

    result
}
