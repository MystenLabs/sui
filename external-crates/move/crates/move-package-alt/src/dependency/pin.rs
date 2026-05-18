// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, path::PathBuf};

use path_clean::PathClean;
use sha2::{Digest, Sha256};
use tracing::debug;

use crate::{
    errors::{FileHandle, PackageError, PackageResult, fmt_truncated},
    flavor::MoveFlavor,
    git::{GitCache, GitError, GitTree},
    package::paths::PackagePath,
    schema::{
        Environment, EnvironmentID, EnvironmentName, EphemeralDependencyInfo, LocalDepInfo,
        LockfileDependencyInfo, LockfileGitDepInfo, ManifestGitDependency, ModeName,
        OnChainDepInfo, PackageName, RenderToml, RootDepInfo,
    },
};

use super::{
    CombinedDependency, DependencyContext,
    combine::Combined,
    resolve::{Resolved, ResolvedDependency},
};

/// A dependency that has been pinned to a specific version. Pairs a [Pinned] source location with
/// a [DependencyContext] describing how a parent references this dependency — this is a graph
/// **edge** concept.
#[derive(Debug, Clone)]
pub struct PinnedDependency {
    context: DependencyContext,
    dep_info: Pinned,
}

/// A pinned source location for a package. Represents where a package's files can be found on
/// disk (or will be fetched to). This is a **node** concept — it describes a package's location,
/// independent of how it is referenced by other packages.
///
/// In contrast, [PinnedDependency] pairs a `Pinned` with a [DependencyContext] and represents a
/// graph **edge** — how a parent package references a dependency.
///
// TODO: consider moving `Pinned` out of the `dependency` module, since it describes package
// locations rather than dependency relationships.
#[derive(Clone, Debug)]
pub(crate) enum Pinned {
    Local(PinnedLocalDependency),
    Git(PinnedGitDependency),
    OnChain(OnChainDepInfo),
    Root(PackagePath),
}

/// Invariant: if a PinnedDependency has `dep_info` `Root`, then its `containing_file` is either a
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

impl PinnedDependency {
    /// Replace all dependencies in `deps` with their pinned versions:
    ///  - first, all external dependencies are resolved (in environment `env`)
    ///  - next, the revisions for git dependencies are replaced with 40-character shas
    ///  - finally, local dependencies are transformed relative to `parent`
    pub(crate) async fn pin<F: MoveFlavor>(
        parent: &Pinned,
        deps: Vec<CombinedDependency>,
        env: &Environment,
        flavor: &F,
    ) -> PackageResult<Vec<PinnedDependency>> {
        debug!("pinning dependencies");
        // replace all system dependencies using the flavor
        let (non_system_deps, mut result) = Self::replace_system_deps(deps, env, flavor).await?;

        // resolution - replace all externally resolved dependencies with internal dependencies
        let deps = ResolvedDependency::resolve(non_system_deps, env).await?;

        // pinning - fix git shas and normalize local deps
        for dep in deps.into_iter() {
            let transformed = match dep.dep_info {
                Resolved::Local(ref loc) => loc.clone().pin(parent, env.id())?,
                Resolved::Git(ref git) => git.pin().await?,
                Resolved::OnChain(_) => todo!(),
            };

            result.push(PinnedDependency {
                context: dep.context,
                dep_info: transformed,
            });
        }

        Ok(result)
    }

    /// Transform a combined dependency into a pinned dependency using the provided pinned
    /// information
    pub(crate) fn from_combined(dep: CombinedDependency, pinned: Pinned) -> Self {
        Self {
            context: dep.context,
            dep_info: pinned,
        }
    }

    /// Partition `deps` into the system dependencies and the non-system dependencies; replace all
    /// the system dependencies using `flavor`.
    async fn replace_system_deps<F: MoveFlavor>(
        deps: Vec<CombinedDependency>,
        env: &Environment,
        flavor: &F,
    ) -> PackageResult<(Vec<CombinedDependency>, Vec<PinnedDependency>)> {
        let all_system_deps = flavor.system_deps(env.id()).await;
        let valid_list = move_compiler::format_oxford_list!(
            "and",
            "{}",
            all_system_deps.keys().collect::<Vec<&String>>()
        );

        let mut system_deps: Vec<PinnedDependency> = Vec::new();
        let mut non_system_deps: Vec<CombinedDependency> = Vec::new();

        for dep in deps.into_iter() {
            if let Combined::System(sys) = &dep.dep_info {
                let lockfile_dep =
                    all_system_deps
                        .get(&sys.system)
                        .ok_or(PackageError::InvalidSystemDep {
                            dep: sys.system.clone(),
                            valid: valid_list.to_string(),
                        })?;
                let file = dep.context.containing_file;
                let pinned_dep = PinnedDependency {
                    context: dep.context,
                    dep_info: Pinned::from_lockfile(file, lockfile_dep)
                        .expect("system dependencies are valid pins"),
                };
                system_deps.push(pinned_dep);
            } else {
                non_system_deps.push(dep);
            }
        }
        Ok((non_system_deps, system_deps))
    }

    /// The name for the dependency
    pub fn name(&self) -> &PackageName {
        &self.context.name
    }

    /// The `use-environment` field for this dependency
    pub fn use_environment(&self) -> &EnvironmentName {
        &self.context.use_environment
    }

    /// The `override` flag for this dependency
    pub fn is_override(&self) -> bool {
        self.context.is_override
    }

    /// The `rename-from` field for this dependency
    pub fn rename_from(&self) -> &Option<PackageName> {
        &self.context.rename_from
    }

    /// Whether this is the root dependency
    pub fn is_root(&self) -> bool {
        self.dep_info.is_root()
    }

    /// The `modes` field for this dependency
    pub fn modes(&self) -> &Option<Vec<ModeName>> {
        &self.context.modes
    }

    /// Access the inner [Pinned] value.
    pub(crate) fn pinned(&self) -> &Pinned {
        &self.dep_info
    }

    /// Return the absolute path to the directory that this dependency would be fetched into.
    pub fn unfetched_path(&self, chain_id: &EnvironmentID) -> PathBuf {
        self.dep_info.unfetched_path(chain_id)
    }
}

impl Pinned {
    /// Is the dependency the root?
    pub(crate) fn is_root(&self) -> bool {
        matches!(self, Pinned::Root(_))
    }

    /// Return the absolute path to the directory that this package would be fetched into, without
    /// actually fetching it. `_chain_id` is unused for now but will be needed for on-chain deps.
    pub(crate) fn unfetched_path(&self, _chain_id: &EnvironmentID) -> PathBuf {
        match &self {
            Pinned::Git(dep) => dep.inner.path_to_tree(),
            Pinned::Local(dep) => dep.absolute_path_to_package.clone(),
            Pinned::OnChain(_dep) => todo!(),
            Pinned::Root(path) => path.path().to_path_buf(),
        }
    }

    /// Create a [Pinned] from a lockfile dependency info entry.
    ///
    /// `containing_file` is the lockfile that the dependency comes from, used to resolve relative
    /// paths.
    pub(crate) fn from_lockfile(
        containing_file: FileHandle,
        pin: &LockfileDependencyInfo,
    ) -> PackageResult<Self> {
        match &pin {
            LockfileDependencyInfo::Local(loc) => Ok(Pinned::Local(PinnedLocalDependency {
                absolute_path_to_package: containing_file
                    .path()
                    .parent()
                    .expect("files have parents")
                    .join(&loc.local)
                    .to_path_buf()
                    .clean(),
                relative_path_from_root_package: loc.local.to_path_buf().clean(),
            })),
            LockfileDependencyInfo::OnChain(chain) => Ok(Pinned::OnChain(chain.clone())),
            LockfileDependencyInfo::Git(git) => Ok(Pinned::Git(git.clone().try_into()?)),
            LockfileDependencyInfo::Root(_) => Ok(Pinned::Root(PackagePath::new(
                containing_file
                    .as_ref()
                    .parent()
                    .expect("files have parents")
                    .to_path_buf(),
            )?)),
        }
    }

    /// Return an abbreviated string (without braces) showing the dependency (e.g. `local = "foo/bar"`)
    pub(crate) fn abbreviated(&self) -> String {
        match &self {
            Pinned::Local(local) => {
                format!(r#"local = {:?}"#, local.relative_path_from_root_package)
            }
            Pinned::Git(git) => {
                let repo = fmt_truncated(git.inner.repo_url(), 8, 12);
                let path = git.inner.path_in_repo().to_string_lossy();
                let rev = fmt_truncated(git.inner.sha(), 6, 2);
                format!(r#"git = "{repo}", path = "{path}", rev = "{rev}""#)
            }
            Pinned::OnChain(_on_chain) => "on-chain = true".to_string(),
            Pinned::Root(_) => "local = \".\"".to_string(),
        }
    }

    /// Returns the default hash for Self.
    pub(crate) fn unique_hash(&self) -> u64 {
        let lockfile_pin: LockfileDependencyInfo = self.clone().into();
        let digest = Sha256::digest(lockfile_pin.render_as_toml().as_bytes());
        // Take the first 8 bytes of the 32-byte SHA256 digest and convert to u64
        let digest_bytes = digest.as_slice();
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&digest_bytes[..8]);
        u64::from_be_bytes(bytes)
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
    /// Takes a local dependency and its `parent`, and transforms local dependencies based on their
    /// parent. `chain_id` is passed through to [Pinned::unfetched_path].
    ///
    /// 1. If the parent is a git dependency, we convert local transitive deps to git.
    /// 2. If the parent is a local dependency, we normalize the path based on the parents.
    fn pin(self, parent: &Pinned, chain_id: &EnvironmentID) -> PackageResult<Pinned> {
        let info: Pinned = match &parent {
            Pinned::Git(parent_git) => Pinned::Git(PinnedGitDependency {
                inner: parent_git.inner.relative_tree(self.local)?,
            }),
            Pinned::Local(parent_local) => Pinned::Local(PinnedLocalDependency {
                absolute_path_to_package: parent.unfetched_path(chain_id).join(&self.local).clean(),
                relative_path_from_root_package: parent_local
                    .relative_path_from_root_package
                    .join(&self.local)
                    .clean(),
            }),
            Pinned::Root(_) => Pinned::Local(PinnedLocalDependency {
                absolute_path_to_package: parent.unfetched_path(chain_id).join(&self.local).clean(),
                relative_path_from_root_package: self.local.clean(),
            }),
            Pinned::OnChain(_) => todo!(),
        };

        Ok(info)
    }
}

impl From<PinnedDependency> for LockfileDependencyInfo {
    fn from(value: PinnedDependency) -> Self {
        value.dep_info.into()
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
            Pinned::Root(_) => Self::Root(RootDepInfo { root: true }),
        }
    }
}

impl Pinned {
    /// Convert to an [EphemeralDependencyInfo]. For ephemeral dependencies, local dependencies
    /// (including the root) are stored as absolute paths.
    pub(crate) fn to_ephemeral(&self, chain_id: &EnvironmentID) -> EphemeralDependencyInfo {
        let local = self
            .unfetched_path(chain_id)
            .canonicalize()
            .expect("Filesystem path for pinned package is valid");

        EphemeralDependencyInfo(LocalDepInfo { local })
    }
}

impl fmt::Display for Pinned {
    // TODO: this is maybe misguided; we should perhaps only display manifest dependencies?
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ {} }}", self.abbreviated())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use std::path::Path;
    use tempfile::{TempDir, tempdir};
    use test_log::test;

    use crate::{schema::GitSha, test_utils::git};

    use super::*;

    const RANDOM_SHA: &str = "1111111111111111111111111111111111111111"; // 40 characters
    const TEST_CHAIN_ID: &str = "test-chain";

    // Local pinning ///////////////////////////////////////////////////////////////////////////////

    /// Pinning a local dep `{local = "../child"}` relative to the root dep returns `{local =
    /// "../child"}` (with the correct absolute path)
    #[test]
    fn local_dep_of_root() {
        let (tempdir, parent) = new_pinned_root("parent");
        assert!(parent.is_root());

        let dep = new_local("../child");
        let pinned = dep
            .pin(&parent, &TEST_CHAIN_ID.to_string())
            .unwrap_as_local()
            .clone();

        assert_eq!(
            pinned.absolute_path_to_package.as_os_str(),
            tempdir.path().join("child")
        );
        assert_eq!(
            pinned.relative_path_from_root_package.as_os_str(),
            "../child"
        );
        drop(tempdir)
    }

    /// Pinning a local dep `{local = "child"}` relative to another local dep `{local = "parent"}`
    /// returns `{local = "parent/child"}`, with the absolute directory set to
    /// `/root/parent/child`
    #[test]
    fn local_dep_of_local() {
        let parent = new_pinned_local_from("/root", "parent");

        let dep = new_local("child");
        let pinned = dep
            .pin(&parent, &TEST_CHAIN_ID.to_string())
            .unwrap_as_local()
            .clone();

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
        let parent = new_pinned_git_from("repo.git", RANDOM_SHA, "parent");
        let dep = new_local("child");

        let pinned = dep.pin(&parent, &TEST_CHAIN_ID.to_string()).unwrap_as_git();
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
        let parent = new_pinned_git_from("repo.git", RANDOM_SHA, "packages/foo");
        let dep = new_local("../bar");

        let pinned = dep.pin(&parent, &TEST_CHAIN_ID.to_string()).unwrap_as_git();
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
        let parent = new_pinned_git_from("repo.git", RANDOM_SHA, "e");
        let dep = new_local("../../d");

        assert_snapshot!(
            dep.pin(&parent, &TEST_CHAIN_ID.to_string()).unwrap_err().to_string(),
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
    async fn git_partial_sha() {
        let git_project = git::new().await;
        let commit = git_project
            .commit(|project| project.add_packages(["a"]))
            .await;

        let repo = git_project.repo_path_str();
        let sha = commit.sha();

        let dep = new_git(repo, Some(&sha[0..12]), "");
        let pinned = dep.pin().await.unwrap_as_git();

        assert_eq!(pinned.inner.repo_url(), git_project.repo_path_str());
        assert_eq!(pinned.inner.sha().as_ref(), sha);
        assert_eq!(pinned.inner.path_in_repo().as_os_str(), ".");
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
        let dep = new_pinned_local_from("", "foo/bar");
        assert_snapshot!(format!("{dep}"), @r###"{ local = "foo/bar" }"###);
    }

    #[test]
    fn display_git() {
        let dep = new_pinned_git_from(
            "https://foo.git.com/org/repo.git",
            "ac4911261dd71cac55cf5bf2dd3288f3a12f2563",
            "foo/bar/baz",
        );
        assert_snapshot!(format!("{dep}"), @r###"{ git = "https://...org/repo.git", path = "foo/bar/baz", rev = "ac4911...63" }"###);
    }

    #[test]
    fn display_root() {
        let (_, dep) = new_pinned_root("");
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

    impl<E: std::fmt::Debug> Helpers for Result<PinnedDependency, E> {
        fn unwrap_as_local(self) -> PinnedLocalDependency {
            self.map(|dep| dep.dep_info).unwrap_as_local()
        }

        fn unwrap_as_git(self) -> PinnedGitDependency {
            self.map(|dep| dep.dep_info).unwrap_as_git()
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

    impl Helpers for PinnedDependency {
        fn unwrap_as_local(self) -> PinnedLocalDependency {
            self.dep_info.unwrap_as_local()
        }

        fn unwrap_as_git(self) -> PinnedGitDependency {
            self.dep_info.unwrap_as_git()
        }
    }

    /// Return a pinned dep for `{ local = "<path>" }`
    fn new_pinned_local_from(pkg_root: impl AsRef<Path>, path: impl AsRef<Path>) -> Pinned {
        Pinned::Local(PinnedLocalDependency {
            absolute_path_to_package: pkg_root.as_ref().join(&path),
            relative_path_from_root_package: path.as_ref().to_path_buf(),
        })
    }

    /// Return a pinned dep for `{ git = "<repo>", rev = "<sha>", path = "<path>" }`. `rev` must be
    /// a 40-character SHA and `path` must not start with `..`
    fn new_pinned_git_from(
        repo: impl AsRef<str>,
        sha: impl AsRef<str>,
        path: impl AsRef<Path>,
    ) -> Pinned {
        let cache = GitCache::new();
        let sha = GitSha::try_from(sha.as_ref().to_string()).expect("valid sha");
        Pinned::Git(PinnedGitDependency {
            inner: cache
                .tree_for_sha(
                    repo.as_ref().to_string(),
                    sha,
                    Some(path.as_ref().to_path_buf()),
                )
                .unwrap(),
        })
    }

    /// Creates a new temporary directory `tmp` containing a dummy `Move.toml` file
    /// `tmp/relative_pkg_path/Move.toml`, and returns a pinned root dependency with the path
    /// `tmp/relative_pkg_path/`.
    fn new_pinned_root(relative_pkg_path: impl AsRef<Path>) -> (TempDir, Pinned) {
        let tempdir = tempdir().unwrap();
        let root_dir = tempdir.path().join(relative_pkg_path.as_ref());
        std::fs::create_dir_all(&root_dir).unwrap();
        std::fs::write(root_dir.join("Move.toml"), "# Dummy Move.toml").unwrap();

        let root = Pinned::Root(
            PackagePath::new(tempdir.path().join(relative_pkg_path.as_ref())).unwrap(),
        );
        (tempdir, root)
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
