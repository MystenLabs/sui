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
//!
//! If you want to then manipulate the repository (such as adding new files or
//! tags), you can use `git2::Repository::open()` to open the repository and then
//! use some of the helper functions in this file to interact with the repository.

use std::fs;
use std::path::{Path, PathBuf};
use url::Url;

use crate::t;
use crate::test_utils::panic_error;
use crate::test_utils::paths::*;
use crate::test_utils::*;
use git2::RepositoryInitOptions;

/// Manually construct a [`Repository`]
///
/// See also [`new`], [`repo`]
#[must_use]
pub struct RepoBuilder {
    repo: git2::Repository,
    files: Vec<PathBuf>,
}

/// See [`new`]
pub struct Repository(git2::Repository);

/// Create a [`RepoBuilder`] to build a new git repository.
///
/// Call [`RepoBuilder::build()`] to finalize and create the repository.
pub fn repo(p: &Path) -> RepoBuilder {
    RepoBuilder::init(p)
}

impl RepoBuilder {
    pub fn init(p: &Path) -> RepoBuilder {
        t!(fs::create_dir_all(p.parent().unwrap()));
        let repo = init(p);
        RepoBuilder {
            repo,
            files: Vec::new(),
        }
    }

    /// Add a file to the repository.
    pub fn file(self, path: &str, contents: &str) -> RepoBuilder {
        let mut me = self.nocommit_file(path, contents);
        me.files.push(PathBuf::from(path));
        me
    }

    /// Add a file that will be left in the working directory, but not added
    /// to the repository.
    pub fn nocommit_file(self, path: &str, contents: &str) -> RepoBuilder {
        let dst = self.repo.workdir().unwrap().join(path);
        t!(fs::create_dir_all(dst.parent().unwrap()));
        t!(fs::write(&dst, contents));
        self
    }

    /// Create the repository and commit the new files.
    pub fn build(self) -> Repository {
        {
            let mut index = t!(self.repo.index());
            for file in self.files.iter() {
                t!(index.add_path(file));
            }
            t!(index.write());
            let id = t!(index.write_tree());
            let tree = t!(self.repo.find_tree(id));
            let sig = t!(self.repo.signature());
            t!(self
                .repo
                .commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[]));
        }
        let RepoBuilder { repo, .. } = self;
        Repository(repo)
    }
}

impl Repository {
    pub fn root(&self) -> &Path {
        self.0.workdir().unwrap()
    }

    pub fn url(&self) -> Url {
        self.0.workdir().unwrap().to_url()
    }

    pub fn revparse_head(&self) -> String {
        self.0
            .revparse_single("HEAD")
            .expect("revparse HEAD")
            .id()
            .to_string()
    }

    pub fn commit(&self) -> git2::Oid {
        commit(&self.0)
    }

    pub fn tag(&self, name: &str) {
        tag(&self.0, name)
    }

    pub fn commits(&self) -> Vec<git2::Commit> {
        commits(&self.0)
    }
}

/// *(`git2`)* Initialize a new repository at the given path.
pub fn init(path: &Path) -> git2::Repository {
    // default_search_path();
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = t!(git2::Repository::init_opts(path, &opts));
    default_repo_cfg(&repo);
    repo
}

fn default_repo_cfg(repo: &git2::Repository) {
    let mut cfg = t!(repo.config());
    t!(cfg.set_str("user.email", "foo@bar.com"));
    t!(cfg.set_str("user.name", "Foo Bar"));
}

/// Create a new [`Project`] in a git [`Repository`]
pub fn new<F>(name: &str, callback: F) -> Project
where
    F: FnOnce(ProjectBuilder) -> ProjectBuilder,
{
    new_repo(name, callback).0
}

/// Create a new [`Project`] with access to the [`Repository`]
pub fn new_repo<F>(name: &str, callback: F) -> (Project, Repository)
where
    F: FnOnce(ProjectBuilder) -> ProjectBuilder,
{
    let mut git_project = project().at(name);
    git_project = callback(git_project);
    let git_project = git_project.build();

    let repo = init(&git_project.root());
    add(&repo);
    commit(&repo);
    (git_project, Repository(repo))
}

/// *(`git2`)* Add all files in the working directory to the git index
pub fn add(repo: &git2::Repository) {
    let mut index = t!(repo.index());
    t!(index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None));
    t!(index.write());
}

/// *(`git2`)* Commit changes to the git repository
fn commit(repo: &git2::Repository) -> git2::Oid {
    let tree_id = t!(t!(repo.index()).write_tree());
    let sig = t!(repo.signature());
    let mut parents = Vec::new();
    if let Some(parent) = repo.head().ok().map(|h| h.target().unwrap()) {
        parents.push(t!(repo.find_commit(parent)))
    }
    let parents = parents.iter().collect::<Vec<_>>();
    t!(repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        "test",
        &t!(repo.find_tree(tree_id)),
        &parents
    ))
}

/// *(`git2`)* Create a new tag in the git repository
fn tag(repo: &git2::Repository, name: &str) {
    let head = repo.head().unwrap().target().unwrap();
    t!(repo.tag(
        name,
        &t!(repo.find_object(head, None)),
        &t!(repo.signature()),
        "make a new tag",
        false
    ));
}

// / *(`git2`)* Get all commits in the repository, starting from HEAD
pub fn commits(repo: &git2::Repository) -> Vec<git2::Commit> {
    let mut revwalk = t!(repo.revwalk());
    t!(revwalk.push_head());
    revwalk
        .map(|oid| t!(repo.find_commit(oid.unwrap())))
        .collect()
}
