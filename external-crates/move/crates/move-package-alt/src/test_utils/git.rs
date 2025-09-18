// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
//
// Copied and adapted from
// <https://github.com/rust-lang/cargo/tree/master/crates/cargo-test-support/src> at SHA
// 4ac865d3d7b62281ad4dcb92406c816b6f1aeceb

//! # Git Testing Support
//!
//! ## Creating a git dependency
//! [`new()`] is an easy way to create a new git repository containing a
//! project that you can then use as a dependency. It will automatically add all
//! the files you specify in the project and commit them to the repository.
//!
//! ### Example:
//!
//! ```no_run
//! let git_project = git::new("dep1", |project| {
//!     project
//!         .file("Move.toml", &basic_manifest("dep1", "1.0.0"))
//!         .file("sources/dep1.move", r#"module dep1::dep1 { public fun f() { } }"#)
//! });
//!
//! // Use the `root()` or `root_path()` method to get the FS path to the new repository.
//! let p = project()
//!     .file("Move.toml", &format!(r#"
//!         [package]
//!         name = "a"
//!         version = "1.0.0"
//!         edition = "2025"
//!
//!         [dependencies]
//!         dep1 = {{ git = '{}' }}
//!     "#, git_project.root_path()))
//!     .file("sources/a.move", "module a::a { public fun t() { dep1::f(); } }")
//!     .build();
//! ```
//!
//! ## Manually creating repositories
//!
//! [`repo()`] can be used to create a [`RepoBuilder`] which provides a way of
//! adding files to a blank repository and committing them.

use crate::git::run_git_cmd_with_args;
use crate::test_utils::*;

/// See [`new`]
pub struct RepoProject(Project);

pub async fn new<F>(name: &str, callback: F) -> RepoProject
where
    F: FnOnce(ProjectBuilder) -> ProjectBuilder,
{
    RepoProject::new(name, callback).await
}

impl RepoProject {
    /// Create a new [`Project`] in a git [`Repository`]
    pub async fn new<F>(name: &str, callback: F) -> Self
    where
        F: FnOnce(ProjectBuilder) -> ProjectBuilder,
    {
        let mut builder = project().at(name);
        builder = callback(builder);
        let project = builder.build();

        let result = Self(project);
        result.init().await;
        result.add_all().await;
        result.commit().await;
        result
    }

    pub async fn commit(&self) -> String {
        self.add_all().await;
        run_git_cmd_with_args(&["commit", "-m", "test commit message"], Some(&self.0.root))
            .await
            .unwrap();
        let mut result = run_git_cmd_with_args(&["rev-parse", "HEAD"], Some(&self.0.root))
            .await
            .unwrap();

        // remove trailing newline
        result.pop();
        result
    }

    pub async fn add_all(&self) {
        run_git_cmd_with_args(&["add", "."], Some(&self.0.root))
            .await
            .unwrap();
    }

    pub async fn tag(&self, name: &str) {
        run_git_cmd_with_args(&["tag", name], Some(&self.0.root))
            .await
            .unwrap();
    }

    pub async fn commits(&self) -> Vec<String> {
        run_git_cmd_with_args(&["reflog", "--format=format:%H"], Some(&self.0.root))
            .await
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }

    async fn init(&self) {
        run_git_cmd_with_args(&["init", "--initial-branch", "main"], Some(&self.0.root))
            .await
            .unwrap();
        run_git_cmd_with_args(&["config", "user.email", "foo@bar.com"], Some(&self.0.root))
            .await
            .unwrap();
        run_git_cmd_with_args(&["config", "user.name", "Foo Bar"], Some(&self.0.root))
            .await
            .unwrap();
    }
}

impl AsRef<Project> for RepoProject {
    fn as_ref(&self) -> &Project {
        &self.0
    }
}
