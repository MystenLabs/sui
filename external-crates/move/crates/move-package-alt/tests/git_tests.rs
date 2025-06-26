// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod test_framework;

use crate::test_framework::basic_manifest;
use crate::test_framework::git;
use crate::test_framework::project;

#[test]
pub fn test() {
    let p = project();

    let git_project = git::new("dep1", |project| {
        project
            .file("Cargo.toml", &basic_manifest("dep1", "0.0.1"))
            .file(
                "source/a.move",
                r#"
                    public fun hello() {}               
                "#,
            )
    });
}
