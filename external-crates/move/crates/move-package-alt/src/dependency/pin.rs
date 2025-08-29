// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt, path::PathBuf};

use path_clean::PathClean;

use crate::{
    dependency::{ResolvedDependency, resolve::Resolved},
    errors::{FileHandle, PackageResult, fmt_truncated},
    flavor::MoveFlavor,
    git::{GitCache, GitError, GitTree},
    schema::{
        EnvironmentID, EnvironmentName, LocalDepInfo, LockfileDependencyInfo, LockfileGitDepInfo,
        ManifestGitDependency, OnChainDepInfo, PackageName, Pin, RootDepInfo,
    },
};

use super::{CombinedDependency, Dependency};

/// [Dependency<Pinned>]s are guaranteed to always resolve to the same package source. For example,
/// a git dependendency with a branch or tag revision may change over time (and is thus not
/// pinned), whereas a git dependency with a sha revision is always guaranteed to produce the same
/// files.
#[derive(Clone, Debug)]
pub(super) enum Pinned {
    Local(PinnedLocalDependency),
    Git(PinnedGitDependency),
    OnChain(OnChainDepInfo),
    Root,
}

/// Invariant: if a PinnedDepencyInfo has `dep_info` `Root`, then its `containing_file` is either a
/// manifest or a lockfile in the directory containing the root package
#[derive(Clone, Debug)]
pub struct PinnedGitDependency {
    pub(crate) inner: GitTree,
}

/// Pinned local dependencies are always relative to the root package, because we normalize the
/// paths during pinning.
#[derive(Clone, Debug, PartialEq)]
pub struct PinnedLocalDependency {
    /// The path to the package directory on the filesystem
    absolute_path_to_package: PathBuf,

    /// The path from the root package to this package, used for serializing the local dependency
    /// back to the root package's lockfile.
    relative_path_from_root_package: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PinnedDependencyInfo(pub(super) Dependency<Pinned>);

impl PinnedDependencyInfo {
    /// Return a dependency representing the root package, of the form
    /// ```toml
    ///     { local = ".", use-environment = "{use_environment}", override = true }
    /// ```
    pub fn root_dependency(containing_file: FileHandle, use_environment: EnvironmentName) -> Self {
        PinnedDependencyInfo(Dependency {
            dep_info: Pinned::Root,
            use_environment,
            is_override: true,
            addresses: None,
            containing_file,
            rename_from: None,
        })
    }

    /// Replace all dependencies in `deps` with their pinned versions:
    ///  - first, all external dependencies are resolved (in environment `environment_id`)
    ///  - next, the revisions for git dependencies are replaced with 40-character shas
    ///  - finally, local dependencies are transformed relative to `parent`
    pub async fn pin<F: MoveFlavor>(
        parent: &PinnedDependencyInfo,
        deps: BTreeMap<PackageName, CombinedDependency>,
        environment_id: &EnvironmentID,
    ) -> PackageResult<BTreeMap<PackageName, PinnedDependencyInfo>> {
        // resolution - replace all externally resolved dependencies with internal dependencies
        let deps = ResolvedDependency::resolve(deps, environment_id).await?;

        // pinning - fix git shas and normalize local deps
        let mut result: BTreeMap<PackageName, PinnedDependencyInfo> = BTreeMap::new();
        for (name, dep) in deps.into_iter() {
            let transformed = match dep.0.dep_info {
                Resolved::Local(ref loc) => loc.clone().pin(parent)?,
                Resolved::Git(ref git) => git.pin().await?,
                Resolved::OnChain(_) => todo!(),
            };

            // TODO: can avoid clones above if we don't use `map` here
            result.insert(name, PinnedDependencyInfo(dep.0.map(|_| transformed)));
        }

        Ok(result)
    }

    /// Create a pinned dependency from a pin in a lockfile. This involves attaching the context of
    /// the file it is contained in (`containing_file`) and the environment it is defined in
    /// (`env`).
    ///
    /// The returned dependency has the `override` field set, since we assume dependencies are
    /// only pinned to the lockfile after the linkage checks have been performed.
    ///
    /// We do not set the `rename-from` field, since when we are creating the pinned dependency we
    /// don't yet know what the rename-from field  should be. The caller is responsible for calling
    /// [Self::with_rename_from] if they need to establish the rename-from check invariant.
    pub fn from_lockfile(
        containing_file: FileHandle,
        env: &EnvironmentName,
        pin: &Pin,
    ) -> PackageResult<Self> {
        let dep_info = match &pin.source {
            LockfileDependencyInfo::Local(loc) => Pinned::Local(PinnedLocalDependency {
                absolute_path_to_package: containing_file
                    .path()
                    .parent()
                    .expect("files have parents")
                    .join(&loc.local)
                    .to_path_buf()
                    .clean(),
                relative_path_from_root_package: loc.local.to_path_buf().clean(),
            }),
            LockfileDependencyInfo::OnChain(chain) => Pinned::OnChain(chain.clone()),
            LockfileDependencyInfo::Git(git) => Pinned::Git(git.clone().try_into()?),
            LockfileDependencyInfo::Root(_) => Pinned::Root,
        };

        Ok(PinnedDependencyInfo(Dependency {
            dep_info,
            use_environment: pin.use_environment.clone().unwrap_or(env.clone()),
            is_override: true,
            addresses: pin.address_override.clone(),
            rename_from: None,
            containing_file,
        }))
    }

    /// The `use-environment` field for this dependency
    pub fn use_environment(&self) -> &EnvironmentName {
        self.0.use_environment()
    }

    /// The `override` flag for this dependency
    pub fn is_override(&self) -> bool {
        self.0.is_override
    }

    /// The `rename-from` field for this dependency
    pub fn rename_from(&self) -> &Option<PackageName> {
        self.0.rename_from()
    }

    /// Is the dependency the root?
    pub fn is_root(&self) -> bool {
        matches!(self.0.dep_info, Pinned::Root)
    }

    /// Return the absolute path to the directory that this package would be fetched into, without
    /// actually fetching it
    pub fn unfetched_path(&self) -> PathBuf {
        match &self.0.dep_info {
            Pinned::Git(dep) => dep.inner.path_to_tree(),
            Pinned::Local(dep) => dep.absolute_path_to_package.clone(),
            Pinned::OnChain(_dep) => todo!(),
            Pinned::Root => {
                // Note: the root dependency should always come from either the lockfile or
                // manifest in the folder containing the root package; we use this to compute the
                // path to the root package
                self.0
                    .containing_file
                    .path()
                    .parent()
                    .expect("files have parents")
                    .to_path_buf()
            }
        }
    }
}

impl ManifestGitDependency {
    /// Replace the commit-ish [self.rev] with a commit (i.e. a SHA). Requires fetching the git
    /// repository
    async fn pin(&self) -> PackageResult<Pinned> {
        let cache = GitCache::new();
        let ManifestGitDependency { repo, rev, subdir } = self.clone();
        let tree = cache.resolve_to_tree(&repo, &rev, Some(subdir)).await?;
        Ok(Pinned::Git(PinnedGitDependency { inner: tree }))
    }
}

impl TryFrom<LockfileGitDepInfo> for PinnedGitDependency {
    type Error = GitError;

    fn try_from(value: LockfileGitDepInfo) -> Result<Self, Self::Error> {
        let cache = GitCache::new();
        let LockfileGitDepInfo { repo, rev, path } = value;
        let tree = cache.tree_for_sha(repo, rev, Some(path))?;
        Ok(PinnedGitDependency { inner: tree })
    }
}

impl LocalDepInfo {
    /// Takes a local dependency, and its parent, and transforms local dependencies
    /// based on their parent.
    /// 1. If the parent is a git dependency, we convert local transitive deps to git.
    /// 2. If the parent is a local dependency, we normalize the path based on the parents.
    fn pin(self, parent: &PinnedDependencyInfo) -> PackageResult<Pinned> {
        let info: Pinned = match &parent.0.dep_info {
            Pinned::Git(parent_git) => Pinned::Git(PinnedGitDependency {
                inner: parent_git.inner.relative_tree(self.local)?,
            }),
            Pinned::Local(parent_local) => Pinned::Local(PinnedLocalDependency {
                absolute_path_to_package: parent.unfetched_path().join(&self.local).clean(),
                relative_path_from_root_package: parent_local
                    .relative_path_from_root_package
                    .join(&self.local)
                    .clean(),
            }),
            Pinned::Root => Pinned::Local(PinnedLocalDependency {
                absolute_path_to_package: parent.unfetched_path().join(&self.local).clean(),
                relative_path_from_root_package: self.local.clean(),
            }),
            Pinned::OnChain(_) => todo!(),
        };

        Ok(info)
    }
}

impl From<PinnedDependencyInfo> for LockfileDependencyInfo {
    fn from(value: PinnedDependencyInfo) -> Self {
        value.0.dep_info.into()
    }
}

impl From<Pinned> for LockfileDependencyInfo {
    fn from(value: Pinned) -> Self {
        match value {
            Pinned::Local(loc) => Self::Local(LocalDepInfo {
                local: loc.relative_path_from_root_package,
            }),
            Pinned::Git(git) => Self::Git(LockfileGitDepInfo {
                repo: git.inner.repo_url().to_string(),
                rev: git.inner.sha().clone(),
                path: git.inner.path_in_repo().to_path_buf(),
            }),
            Pinned::OnChain(on_chain) => Self::OnChain(on_chain),
            Pinned::Root => Self::Root(RootDepInfo { root: true }),
        }
    }
}

impl fmt::Display for PinnedDependencyInfo {
    // TODO: this is maybe misguided; we should perhaps only display manifest dependencies?
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0.dep_info {
            Pinned::Local(_local) => write!(
                f,
                r#"{{ local = {:?} }}"#,
                _local.relative_path_from_root_package
            ),
            Pinned::Git(git) => {
                let repo = fmt_truncated(git.inner.repo_url(), 8, 12);
                let path = git.inner.path_in_repo().to_string_lossy();
                let rev = fmt_truncated(git.inner.sha(), 6, 2);
                write!(f, r#"{{ git = "{repo}", path = "{path}", rev = "{rev}" }}"#)
            }
            Pinned::OnChain(_on_chain) => write!(f, "{{ on-chain = true }}"),
            Pinned::Root => write!(f, "{{ local = \".\" }}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use std::path::Path;
    use test_log::test;

    use crate::{schema::GitSha, test_utils::git};

    use super::*;

    const RANDOM_SHA: &str = "1111111111111111111111111111111111111111"; // 40 characters

    // Local pinning ///////////////////////////////////////////////////////////////////////////////

    /// Pinning a local dep `{local = "../child"}` relative to the root dep returns `{local =
    /// "../child"}` (with the correct absolute path)
    #[test]
    fn local_dep_of_root() {
        let parent = new_pinned_root("/root/Move.lock");
        assert!(parent.is_root());

        let dep = new_local("../child");
        let pinned = dep.pin(&parent).unwrap_as_local().clone();

        assert_eq!(pinned.absolute_path_to_package.as_os_str(), "/child");
        assert_eq!(
            pinned.relative_path_from_root_package.as_os_str(),
            "../child"
        );
    }

    /// Pinning a local dep `{local = "child"}` relative to another local dep `{local = "parent"}`
    /// returns `{local = "parent/child"}`, with the absoluted directory set to
    /// `/root/parent/child`
    #[test]
    fn local_dep_of_local() {
        let parent = new_pinned_local_from("/root/Move.lock", "parent");

        let dep = new_local("child");
        let pinned = dep.pin(&parent).unwrap_as_local().clone();

        assert_eq!(
            pinned.absolute_path_to_package.as_os_str(),
            "/root/parent/child"
        );
        assert_eq!(
            pinned.relative_path_from_root_package.as_os_str(),
            "parent/child"
        );
    }

    /// Pinning a local dep `{local = "child"}`
    /// relative to a git dep `{git = "repo.git", rev = "...", subdir = "parent"}`
    /// returns `{git = "repo.git", rev = "...", subdir = "parent/child"}`
    #[test]
    fn local_dep_of_git() {
        let parent = new_pinned_git_from("/root/Move.lock", "repo.git", RANDOM_SHA, "parent");
        let dep = new_local("child");

        let pinned = dep.pin(&parent).unwrap_as_git();
        let parent_git = parent.unwrap_as_git();

        assert_eq!(
            // repo url is taken from parent
            pinned.inner.repo_url(),
            parent_git.inner.repo_url()
        );

        assert_eq!(
            // repo sha is taken from parent
            pinned.inner.sha(),
            parent_git.inner.sha()
        );

        assert_eq!(
            // path in repo is updated
            pinned.inner.path_in_repo().as_os_str(),
            "parent/child"
        );
    }

    /// Pinning a local dep `{local = "../bar"}`
    /// relative to a git dep `{git = "repo.git", rev = "...", subdir = "packages/foo"}`
    /// returns `{git = "repo.git", rev = "...", subdir = "packages/bar"}`
    #[test]
    fn local_dep_of_git_with_subdir() {
        let parent = new_pinned_git_from("/root/Move.lock", "repo.git", RANDOM_SHA, "packages/foo");
        let dep = new_local("../bar");

        let pinned = dep.pin(&parent).unwrap_as_git();
        let parent_git = parent.unwrap_as_git();

        assert_eq!(pinned.inner.repo_url(), parent_git.inner.repo_url());
        assert_eq!(pinned.inner.sha(), parent_git.inner.sha());
        assert_eq!(pinned.inner.path_in_repo().as_os_str(), "packages/bar");
    }

    /// Pinning a local dep `{local = "../../d"}`
    /// relative to a git dep `{ git = "repo.git", rev = "...", subdir = "e"}`
    /// returns an error
    #[test]
    fn local_dep_outside_of_git() {
        let parent = new_pinned_git_from("/root/Move.lock", "repo.git", RANDOM_SHA, "e");
        let dep = new_local("../../d");

        assert_snapshot!(
            dep.pin(&parent).unwrap_err().to_string(),
            @"relative path `../d` is not contained in the repository"
        );
    }

    // Git pinning /////////////////////////////////////////////////////////////////////////////////

    /// Pinning a git dep with a full SHA returns it unchanged; the remote is not contacted
    #[test(tokio::test)]
    async fn git_full_sha() {
        let dep = new_git("child.git", Some(RANDOM_SHA), ".");
        let pinned = dep.pin().await.unwrap_as_git();

        assert_eq!(pinned.inner.repo_url(), "child.git");
        assert_eq!(pinned.inner.sha().as_ref(), RANDOM_SHA);
        assert_eq!(pinned.inner.path_in_repo().as_os_str(), ".");
    }

    /// Pinning a git dep with a partial SHA expands it to 40 characters
    #[test(tokio::test)]
    #[ignore] // TODO
    async fn git_partial_sha() {
        let git_project =
            git::new("git_project", |project| project.file("dummy.txt", "dummy")).await;

        let repo = "child.git"; // TODO: get repo from git_project
        let sha = git_project.commits().await.remove(0);

        let dep = new_git(repo, Some(&sha[0..12]), "");
        let pinned = dep.pin().await.unwrap_as_git();

        assert_eq!(pinned.inner.repo_url(), "child.git");
        assert_eq!(pinned.inner.sha().as_ref(), sha);
        assert_eq!(pinned.inner.path_in_repo().as_os_str(), "");
    }

    /// Pinning a git dep with a branch converts it to a sha
    #[test(tokio::test)]
    #[ignore]
    async fn git_branch() {
        todo!()
    }

    /// Pinning a git dep with a tag converts it to a sha
    #[test(tokio::test)]
    #[ignore]
    async fn git_tag() {
        todo!()
    }

    /// Pinning a git dep with no rev converts it to the SHA of the main branch
    #[test(tokio::test)]
    #[ignore]
    async fn git_no_rev() {
        todo!()
    }

    // Displaying pinned deps //////////////////////////////////////////////////////////////////////

    #[test]
    fn display_local() {
        let dep = new_pinned_local_from("Move.lock", "foo/bar");
        assert_snapshot!(format!("{dep}"), @r###"{ local = "foo/bar" }"###);
    }

    #[test]
    fn display_git() {
        let dep = new_pinned_git_from(
            "Move.lock",
            "https://foo.git.com/org/repo.git",
            "ac4911261dd71cac55cf5bf2dd3288f3a12f2563",
            "foo/bar/baz",
        );
        assert_snapshot!(format!("{dep}"), @r###"{ git = "https://...org/repo.git", path = "foo/bar/baz", rev = "ac4911...63" }"###);
    }

    #[test]
    fn display_root() {
        let dep = new_pinned_root("Move.lock");
        assert_snapshot!(format!("{dep}"), @r###"{ local = "." }"###);
    }

    // Test infrastructure /////////////////////////////////////////////////////////////////////////

    /// (unsafe) convenience methods for specializing Pinned values to particular variants
    trait Helpers {
        fn unwrap_as_local(self) -> PinnedLocalDependency;
        fn unwrap_as_git(self) -> PinnedGitDependency;
    }

    impl<E: std::fmt::Debug> Helpers for Result<Pinned, E> {
        fn unwrap_as_local(self) -> PinnedLocalDependency {
            self.unwrap().unwrap_as_local()
        }

        fn unwrap_as_git(self) -> PinnedGitDependency {
            self.unwrap().unwrap_as_git()
        }
    }

    impl<E: std::fmt::Debug> Helpers for Result<PinnedDependencyInfo, E> {
        fn unwrap_as_local(self) -> PinnedLocalDependency {
            self.map(|dep| dep.0.dep_info).unwrap_as_local()
        }

        fn unwrap_as_git(self) -> PinnedGitDependency {
            self.map(|dep| dep.0.dep_info).unwrap_as_git()
        }
    }

    impl Helpers for Pinned {
        fn unwrap_as_local(self) -> PinnedLocalDependency {
            match self {
                Pinned::Local(loc) => loc,
                _ => panic!("Expected local dep"),
            }
        }

        fn unwrap_as_git(self) -> PinnedGitDependency {
            match self {
                Pinned::Git(git) => git,
                _ => panic!("Expected git dep"),
            }
        }
    }

    impl Helpers for PinnedDependencyInfo {
        fn unwrap_as_local(self) -> PinnedLocalDependency {
            self.0.dep_info.unwrap_as_local()
        }

        fn unwrap_as_git(self) -> PinnedGitDependency {
            self.0.dep_info.unwrap_as_git()
        }
    }

    /// Return a pinned dep for `{ local = "<path>" }`
    fn new_pinned_local_from(
        root_lockfile: impl AsRef<Path>,
        path: impl AsRef<Path>,
    ) -> PinnedDependencyInfo {
        let info = LocalDepInfo {
            local: path.as_ref().to_path_buf(),
        };
        new_pin(root_lockfile, LockfileDependencyInfo::Local(info))
    }

    /// Return a pinned dep for `{ git = "<repo>", rev = "<sha>", path = "<path>" }`. `rev` must be
    /// a 40-character SHA and `path` must not start with `..`
    fn new_pinned_git_from(
        root_lockfile: impl AsRef<Path>,
        repo: impl AsRef<str>,
        sha: impl AsRef<str>,
        path: impl AsRef<Path>,
    ) -> PinnedDependencyInfo {
        let info = LockfileGitDepInfo {
            repo: repo.as_ref().to_string(),
            rev: GitSha::try_from(sha.as_ref().to_string()).expect("valid sha"),
            path: path.as_ref().to_path_buf(),
        };

        new_pin(root_lockfile, LockfileDependencyInfo::Git(info))
    }

    fn new_pinned_root(root_lockfile: impl AsRef<Path>) -> PinnedDependencyInfo {
        PinnedDependencyInfo::root_dependency(
            FileHandle::dummy(root_lockfile, ""),
            EnvironmentName::from("test_env"),
        )
    }

    /// Wrap a lockfile dependency into a PinnedDependencyInfo by manufacturing context
    fn new_pin(
        root_lockfile: impl AsRef<Path>,
        source: LockfileDependencyInfo,
    ) -> PinnedDependencyInfo {
        PinnedDependencyInfo::from_lockfile(
            FileHandle::dummy(root_lockfile, ""),
            &EnvironmentName::from("test_env"),
            &Pin {
                source,
                address_override: None,
                use_environment: None,
                manifest_digest: "".into(),
                deps: BTreeMap::new(),
            },
        )
        .unwrap()
    }

    fn new_local(path: impl AsRef<Path>) -> LocalDepInfo {
        LocalDepInfo {
            local: path.as_ref().to_path_buf(),
        }
    }

    fn new_git(
        repo: impl AsRef<str>,
        rev: Option<impl AsRef<str>>,
        subdir: impl AsRef<Path>,
    ) -> ManifestGitDependency {
        ManifestGitDependency {
            repo: repo.as_ref().to_string(),
            rev: rev.map(|rev| rev.as_ref().to_string()),
            subdir: subdir.as_ref().to_path_buf(),
        }
    }
}
