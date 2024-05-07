// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `build.rs`

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut project_path = std::path::PathBuf::from(&manifest);
    project_path.push("boilerplate");
    let project_path = project_path.as_path();

    if project_path.exists() && project_path.is_file() {
        std::fs::remove_file(project_path).unwrap();
        std::fs::create_dir_all(project_path).unwrap();
        return;
    }

    if !project_path.exists() {
        std::fs::create_dir_all(project_path).unwrap();
    }
}
