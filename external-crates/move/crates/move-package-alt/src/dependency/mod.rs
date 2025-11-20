// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use tempfile::tempdir;

mod combine;
pub use combine::CombinedDependency;

mod resolve;
pub use resolve::{ResolvedDependency, ResolverError};

mod pin;
pub use pin::Pinned;
pub use pin::PinnedDependencyInfo;

mod fetch;
pub use fetch::{FetchError, FetchedDependency};

use crate::errors::PackageResult;
use crate::flavor::MoveFlavor;
use crate::package::Package;
use crate::package::paths::PackagePath;
use crate::schema::CachedPackageInfo;
use crate::schema::DefaultDependency;
use crate::schema::Environment;
use crate::schema::ManifestDependencyInfo;
use crate::{
    errors::FileHandle,
    schema::{EnvironmentName, ModeName, PackageName, PublishAddresses},
};

// TODO(refactor): instead of `Dependency<DepInfo>`, we should just have `DependencyContext`, and
// the dependency types will hold one of those and pass it around.

/// [Dependency] wraps information about the location of a dependency (such as the `git` or `local`
/// fields) with additional metadata about how the dependency is used (such as the source file,
/// enviroment overrides, etc).
///
/// At different stages of the pipeline we have different information about the dependency location
/// (e.g. resolved dependencies have no `External` variant, pinned dependencies have a pinned git
/// dependency, etc). The `DepInfo` type encapsulates these invariants.
#[derive(Debug, Clone)]
struct Dependency<DepInfo> {
    /// The name given to this dependency in the manifest. For modern manifests, this is the same
    /// as the name used for the package in the source code, while for legacy manifests this name
    /// may be different (it is still normalized to be a valid identifier but does not correspond
    /// to the named address).
    name: PackageName,

    dep_info: DepInfo,

    /// The environment in the dependency's namespace to use. For example, given
    /// ```toml
    /// dep-replacements.mainnet.foo = { ..., use-environment = "testnet" }
    /// ```
    /// `use_environment` variable would be `testnet`
    use_environment: EnvironmentName,

    /// The `rename-from` field for the dependency
    rename_from: Option<PackageName>,

    /// Was this dependency written with `override = true` in its original manifest?
    is_override: bool,

    /// Does the original manifest override the published address?
    addresses: Option<PublishAddresses>,

    /// The `modes` field for the dependency
    modes: Option<Vec<ModeName>>,

    /// What manifest or lockfile does this dependency come from?
    containing_file: FileHandle,
}

/// Ensure that the dependency given by `dep_info` is cached on disk, and return information
/// about its publication in `env`
pub async fn cache_package<F: MoveFlavor>(
    env: &Environment,
    manifest_dep: &ManifestDependencyInfo,
) -> PackageResult<CachedPackageInfo> {
    // We need some file handles and things to give context to the dep loading system
    let tempdir = tempdir().expect("can create a temporary directory");
    let toml_path = tempdir.path().join("Move.toml");
    std::fs::write(&toml_path, "").expect("can write to temporary file");

    let toml_handle = FileHandle::new(toml_path).expect("can load a newly created tempfile");
    let dummy_path = PackagePath::new(tempdir.path().to_path_buf())
        .expect("temporary directory is a valid package");

    let mtx = dummy_path.lock().expect("can lock the temporary directory");
    let package = PackageName::new("unknown").expect("`unknown` is a valid identifier");

    // Create the manifest dependency
    let default_dep = DefaultDependency {
        dependency_info: manifest_dep.clone(),
        is_override: false,
        rename_from: None,
        modes: None,
    };

    // convert to a combined dependency
    let combined =
        CombinedDependency::from_default(toml_handle, package, env.name().clone(), default_dep);

    // pin
    let root = Pinned::Root(dummy_path);
    let deps = PinnedDependencyInfo::pin::<F>(&root, vec![combined], env.id()).await?;

    // load
    let package = Package::<F>::load(deps[0].as_ref().clone(), env, &mtx).await?;

    // summarize
    Ok(CachedPackageInfo {
        name: package.name().clone(),
        addresses: package.publication().map(|p| p.addresses.clone()),
        chain_id: env.id.clone(),
    })
}

impl<T> Dependency<T> {
    /// Apply `f` to `self.dep_info`, keeping the remaining fields unchanged
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Dependency<U> {
        Dependency {
            name: self.name,
            dep_info: f(self.dep_info),
            use_environment: self.use_environment,
            is_override: self.is_override,
            addresses: self.addresses,
            containing_file: self.containing_file,
            rename_from: self.rename_from,
            modes: self.modes,
        }
    }

    pub fn use_environment(&self) -> &EnvironmentName {
        &self.use_environment
    }

    pub fn rename_from(&self) -> &Option<PackageName> {
        &self.rename_from
    }
}

#[cfg(test)]
mod test {
    use test_log::test;

    use crate::{
        cache_package,
        flavor::{
            Vanilla,
            vanilla::{DEFAULT_ENV_ID, default_environment},
        },
        schema::{
            CachedPackageInfo, LocalDepInfo, ManifestDependencyInfo, OriginalID, PublishAddresses,
            PublishedID,
        },
        test_utils::graph_builder::TestPackageGraph,
    };

    /// Create a basic package and then call cache_package on a local dependency to it; check that
    /// the returned fields are correct
    #[test(tokio::test)]
    async fn test_cache_package() {
        let scenario = TestPackageGraph::new(["root"])
            .add_published("a", OriginalID::from(1), PublishedID::from(2))
            .build();

        let path = scenario.path_for("a");
        let env = default_environment();
        let dep = &ManifestDependencyInfo::Local(LocalDepInfo { local: path });

        let info = cache_package::<Vanilla>(&env, dep).await.unwrap();

        let CachedPackageInfo {
            name,
            addresses,
            chain_id,
        } = info;

        let PublishAddresses {
            published_at,
            original_id,
        } = addresses.unwrap();

        assert_eq!(name.as_str(), "a");
        assert_eq!(published_at, PublishedID::from(2));
        assert_eq!(original_id, OriginalID::from(1));
        assert_eq!(chain_id, DEFAULT_ENV_ID);
    }
}
