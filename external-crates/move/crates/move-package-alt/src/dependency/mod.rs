// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod dependency_set;
mod external;
mod git;
mod local;

pub use dependency_set::DependencySet;
pub mod external_protocol;

use std::{
    collections::BTreeMap,
    fmt::Debug,
    path::PathBuf,
    process::{Command, Stdio},
};

use derive_where::derive_where;
use serde::{Deserialize, Serialize};

use crate::{
    errors::{GitError, PackageError, PackageResult, ResolverError},
    flavor::MoveFlavor,
    package::{EnvironmentName, PackageName},
};

use external::ExternalDependency;
use git::{GitRepo, PinnedGitDependency, UnpinnedGitDependency};
use local::LocalDependency;

/// Phantom type to represent pinned dependencies (see [PinnedDependency])
#[derive(Debug)]
pub struct Pinned;

/// Phantom type to represent unpinned dependencies (see [ManifestDependencyInfo])
#[derive(Debug, PartialEq, Eq)]
pub struct Unpinned;

/// [ManifestDependencyInfo]s contain the dependency-type-specific things that users write in their
/// Move.toml files in the `dependencies` section.
///
/// There are additional general fields in the manifest format (like `override` or `rename-from`)
/// that are not part of the ManifestDependencyInfo. We separate these partly because these things
/// are not serialized to the Lock file. See [crate::package::manifest] for the full representation
/// of an entry in the `dependencies` table.
#[derive(Debug, Serialize, Deserialize)]
#[derive_where(Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum ManifestDependencyInfo<F: MoveFlavor> {
    Git(UnpinnedGitDependency),
    External(ExternalDependency),
    Local(LocalDependency),
    FlavorSpecific(F::FlavorDependency<Unpinned>),
}

/// Pinned dependencies are guaranteed to always resolve to the same package source. For example,
/// a git dependendency with a branch or tag revision may change over time (and is thus not
/// pinned), whereas a git dependency with a sha revision is always guaranteed to produce the same
/// files.
///
/// Local dependencies are a somewhat special case here - we want to pin them as local deps during
/// development, because the developer would expect to use the latest code without having to
/// explicitly repin, but we need to convert them to persistent dependencies when we publish since
/// we want to retain that information for source verification.
#[derive(Debug, Serialize, Deserialize)]
#[derive_where(Clone)]
#[serde(untagged)]
pub enum PinnedDependencyInfo<F: MoveFlavor + ?Sized> {
    Git(PinnedGitDependency),
    Local(LocalDependency),
    FlavorSpecific(F::FlavorDependency<Pinned>),
}

/// Split up deps into kinds. The union of the output sets is the same as [deps]
#[allow(clippy::type_complexity)]
fn split<F: MoveFlavor>(
    deps: &DependencySet<ManifestDependencyInfo<F>>,
) -> (
    DependencySet<UnpinnedGitDependency>,
    DependencySet<ExternalDependency>,
    DependencySet<LocalDependency>,
    DependencySet<F::FlavorDependency<Unpinned>>,
) {
    let mut gits = DependencySet::new();
    let mut exts = DependencySet::new();
    let mut locs = DependencySet::new();
    let mut flav = DependencySet::new();

    for (env, package_name, dep) in deps.clone().into_iter() {
        match dep {
            ManifestDependencyInfo::Git(info) => gits.insert(env, package_name, info),
            ManifestDependencyInfo::External(info) => exts.insert(env, package_name, info),
            ManifestDependencyInfo::Local(info) => locs.insert(env, package_name, info),
            ManifestDependencyInfo::FlavorSpecific(info) => flav.insert(env, package_name, info),
        }
    }

    (gits, exts, locs, flav)
}

/// Replace all dependencies with their pinned versions. The returned set may have a different set
/// of keys than the input, for example if new implicit dependencies are added or if external
/// resolvers resolve default deps to dep-overrides, or if dep-overrides are identical to the
/// default deps.
pub async fn pin<F: MoveFlavor>(
    flavor: &F,
    deps: &DependencySet<ManifestDependencyInfo<F>>,
    envs: &BTreeMap<EnvironmentName, F::EnvironmentID>,
) -> PackageResult<DependencySet<PinnedDependencyInfo<F>>> {
    let (mut gits, mut exts, mut locs, mut flav) = split(deps);

    // resolution
    let resolved = ExternalDependency::resolve::<F>(exts, envs).await?;
    let (resolved_gits, resolved_exts, resolved_locs, resolved_flav) = split(&resolved);
    assert!(resolved_exts.is_empty(), "resolve() returns resolved deps");

    gits.extend(resolved_gits);
    locs.extend(resolved_locs);
    flav.extend(resolved_flav);

    // Check first if git is installed
    if Command::new("git")
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .is_err()
    {
        return Err(PackageError::Git(GitError::not_found()));
    }

    // pinning
    let pinned_gits: DependencySet<PinnedDependencyInfo<F>> = UnpinnedGitDependency::pin(gits)
        .await?
        .into_iter()
        .map(|(env, package, dep)| (env, package, PinnedDependencyInfo::Git::<F>(dep)))
        .collect();

    let pinned_locs = locs
        .into_iter()
        .map(|(env, package, dep)| (env, package, PinnedDependencyInfo::Local::<F>(dep)))
        .collect();

    let pinned_flav = flavor
        .pin(flav)?
        .into_iter()
        .map(|(env, package, dep)| {
            (
                env,
                package,
                PinnedDependencyInfo::FlavorSpecific::<F>(dep.clone()),
            )
        })
        .collect();

    Ok(DependencySet::merge([
        pinned_gits,
        pinned_locs,
        pinned_flav,
    ]))
}

/// For each environment, if none of the implicit dependencies are present in [deps] (or the
/// default environment), then they are all added.
// TODO: what's the notion of identity used here?
fn add_implicit_deps<F: MoveFlavor>(
    flavor: &F,
    deps: &mut DependencySet<PinnedDependencyInfo<F>>,
) -> PackageResult<()> {
    todo!()
}

/// Ensure that all dependencies are stored locally and return the paths to their contents. The
/// returned map is guaranteed to have the same keys as [deps].
fn fetch<F: MoveFlavor>(
    deps: DependencySet<PinnedDependencyInfo<F>>,
) -> PackageResult<DependencySet<PathBuf>> {
    // TODO: check if dependency is a Move project.

    todo!()
}

// Check dependency is a Move project
// fn check_is_move_project<F: MoveFlavor>(
//     deps: DependencySet<PinnedDependencyInfo<F>>,
// ) -> PackageResult<()> {
//     for (defaults, overrides) in deps.iter() {
//         for (env, dep) in overrides {
//             check_is_move_project_impl::<F>(defaults, env, dep)?;
//         }
//     }
//     let move_toml_path = repo_fs_path.join(path).join("Move.toml");
//     debug!("Move toml path: {:?}", move_toml_path.display());
//     let cmd = Command::new("git")
//         .current_dir(&repo_fs_path)
//         .arg("ls-tree")
//         .arg("HEAD")
//         .arg(&move_toml_path)
//         .stdin(Stdio::null())
//         .output()
//         .map_err(|e| {
//             PackageError::Git(GitError::generic(
//                 "Failed to call git ls-tree: {e}".to_string(),
//             ))
//         })?;
//
//     if cmd.stdout.is_empty() {
//         return Err(PackageError::Git(GitError::not_move_project(
//             &self.repo_url,
//         )));
//     }
//
//     Ok(())
// }
