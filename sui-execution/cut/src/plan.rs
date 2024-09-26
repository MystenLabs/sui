// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use toml::value::Value;
use toml_edit::{self, Document, Item};

use crate::args::Args;
use crate::path::{deep_copy, normalize_path, path_relative_to, shortest_new_prefix};

/// Description of where packages should be copied to, what their new names should be, and whether
/// they should be added to the `workspace` `members` or `exclude` fields.
#[derive(Debug)]
pub(crate) struct CutPlan {
    /// Root of the repository, where the `Cargo.toml` containing the `workspace` configuration is
    /// found.
    root: PathBuf,

    /// New directories that need to be created.  Used to clean-up copied packages on roll-back.  If
    /// multiple nested directories must be created, only contains their shortest common prefix.
    directories: BTreeSet<PathBuf>,

    /// Mapping from the names of existing packages to be cut, to the details of where they will be
    /// copied to.
    packages: BTreeMap<String, CutPackage>,
}

/// Details for an individual copied package in the feature being cut.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CutPackage {
    dst_name: String,
    src_path: PathBuf,
    dst_path: PathBuf,
    ws_state: WorkspaceState,
}

/// Whether the package in question is an explicit member of the workspace, an explicit exclude of
/// the workspace, or neither (in which case it could still transitively be one or the other).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceState {
    Member,
    Exclude,
    Unknown,
}

/// Relevant contents of a Cargo.toml `workspace` section.
#[derive(Debug)]
struct Workspace {
    /// Canonicalized paths of workspace members
    members: HashSet<PathBuf>,
    /// Canonicalized paths of workspace excludes
    exclude: HashSet<PathBuf>,
}

#[derive(Error, Debug)]
pub(crate) enum Error {
    #[error("Could not find repository root, please supply one")]
    NoRoot,

    #[error("No [workspace] found at {}/Cargo.toml", .0.display())]
    NoWorkspace(PathBuf),

    #[error("Both member and exclude of [workspace]: {}", .0.display())]
    WorkspaceConflict(PathBuf),

    #[error("Packages '{0}' and '{1}' map to the same cut package name")]
    PackageConflictName(String, String),

    #[error("Packages '{0}' and '{1}' map to the same cut package path")]
    PackageConflictPath(String, String),

    #[error("Cutting package '{0}' will overwrite existing path: {}", .1.display())]
    ExistingPackage(String, PathBuf),

    #[error("'{0}' field is not an array of strings")]
    NotAStringArray(&'static str),

    #[error("Cannot represent path as a TOML string: {}", .0.display())]
    PathToTomlStr(PathBuf),
}

impl CutPlan {
    /// Scan `args.directories` looking for `args.packages` to produce a new plan.  The resulting
    /// plan is guaranteed not to contain any duplicate packages (by name or path), or overwrite any
    /// existing packages.  Returns an error if it's not possible to construct such a plan.
    pub(crate) fn discover(args: Args) -> Result<Self> {
        let cwd = env::current_dir()?;

        let Some(root) = args.root.or_else(|| discover_root(cwd)) else {
            bail!(Error::NoRoot);
        };

        let root = fs::canonicalize(root)?;

        struct Walker {
            feature: String,
            ws: Option<Workspace>,
            planned_packages: BTreeMap<String, CutPackage>,
            pending_packages: HashSet<String>,
            make_directories: BTreeSet<PathBuf>,
        }

        impl Walker {
            fn walk(
                &mut self,
                src: &Path,
                dst: &Path,
                suffix: &Option<String>,
                mut fresh_parent: bool,
            ) -> Result<()> {
                self.try_insert_package(src, dst, suffix)
                    .with_context(|| format!("Failed to plan copy for {}", src.display()))?;

                // Figure out whether the parent directory was already created, or whether this
                // directory needs to be created.
                if !fresh_parent && !dst.exists() {
                    self.make_directories.insert(dst.to_owned());
                    fresh_parent = true;
                }

                for entry in fs::read_dir(src)? {
                    let entry = entry?;
                    if !entry.file_type()?.is_dir() {
                        continue;
                    }

                    // Skip `target` directories.
                    if entry.file_name() == "target" {
                        continue;
                    }

                    self.walk(
                        &src.join(entry.file_name()),
                        &dst.join(entry.file_name()),
                        suffix,
                        fresh_parent,
                    )?;
                }

                Ok(())
            }

            fn try_insert_package(
                &mut self,
                src: &Path,
                dst: &Path,
                suffix: &Option<String>,
            ) -> Result<()> {
                let toml = src.join("Cargo.toml");

                let Some(pkg_name) = package_name(toml)? else {
                    return Ok(());
                };

                if !self.pending_packages.remove(&pkg_name) {
                    return Ok(());
                }

                let mut dst_name = suffix
                    .as_ref()
                    .and_then(|s| pkg_name.strip_suffix(s))
                    .unwrap_or(&pkg_name)
                    .to_string();

                dst_name.push('-');
                dst_name.push_str(&self.feature);

                let dst_path = dst.to_path_buf();
                if dst_path.exists() {
                    bail!(Error::ExistingPackage(pkg_name, dst_path));
                }

                self.planned_packages.insert(
                    pkg_name,
                    CutPackage {
                        dst_name,
                        dst_path,
                        src_path: src.to_path_buf(),
                        ws_state: if let Some(ws) = &self.ws {
                            ws.state(src)?
                        } else {
                            WorkspaceState::Unknown
                        },
                    },
                );

                Ok(())
            }
        }

        let mut walker = Walker {
            feature: args.feature,
            ws: if args.workspace_update {
                Some(Workspace::read(&root)?)
            } else {
                None
            },
            planned_packages: BTreeMap::new(),
            pending_packages: args.packages.into_iter().collect(),
            make_directories: BTreeSet::new(),
        };

        for dir in args.directories {
            let src_path = fs::canonicalize(&dir.src)
                .with_context(|| format!("Canonicalizing {} failed", dir.src.display()))?;

            // Remove redundant `..` components from the destination path to avoid creating
            // directories we may not need at the destination.  E.g. a destination path of
            //
            //   foo/../bar
            //
            // Should only create the directory `bar`, not also the directory `foo`.
            let dst_path = normalize_path(&dir.dst)
                .with_context(|| format!("Normalizing {} failed", dir.dst.display()))?;

            // Check whether any parent directories need to be made as part of this iteration of the
            // cut.
            let fresh_parent = shortest_new_prefix(&dst_path).map_or(false, |pfx| {
                walker.make_directories.insert(pfx);
                true
            });

            walker
                .walk(
                    &fs::canonicalize(dir.src)?,
                    &dst_path,
                    &dir.suffix,
                    fresh_parent,
                )
                .with_context(|| format!("Failed to find packages in {}", src_path.display()))?;
        }

        // Emit warnings for packages that were not found
        for pending in &walker.pending_packages {
            eprintln!("WARNING: Package '{pending}' not found during scan.");
        }

        let Walker {
            planned_packages: packages,
            make_directories: directories,
            ..
        } = walker;

        //  Check for conflicts in the resulting plan
        let mut rev_name = HashMap::new();
        let mut rev_path = HashMap::new();

        for (name, pkg) in &packages {
            if let Some(prev) = rev_name.insert(pkg.dst_name.clone(), name.clone()) {
                bail!(Error::PackageConflictName(name.clone(), prev));
            }

            if let Some(prev) = rev_path.insert(pkg.dst_path.clone(), name.clone()) {
                bail!(Error::PackageConflictPath(name.clone(), prev));
            }
        }

        Ok(Self {
            root,
            packages,
            directories,
        })
    }

    /// Copy the packages according to this plan.  On success, all the packages will be copied to
    /// their destinations, and their dependencies will be fixed up.  On failure, pending changes
    /// are rolled back.
    pub(crate) fn execute(&self) -> Result<()> {
        self.execute_().inspect_err(|_| {
            self.rollback();
        })
    }
    fn execute_(&self) -> Result<()> {
        for (name, package) in &self.packages {
            self.copy_package(package).with_context(|| {
                format!("Failed to copy package '{name}' to '{}'.", package.dst_name)
            })?
        }

        for package in self.packages.values() {
            self.update_package(package)
                .with_context(|| format!("Failed to update manifest for '{}'", package.dst_name))?
        }

        // Update the workspace at the end, so that if there is any problem before that, rollback
        // will leave the state clean.
        self.update_workspace()
            .context("Failed to update [workspace].")
    }

    /// Copy the contents of `package` from its `src_path` to its `dst_path`, unchanged.
    fn copy_package(&self, package: &CutPackage) -> Result<()> {
        // Copy everything in the directory as-is, except for any "target" directories
        deep_copy(&package.src_path, &package.dst_path, &mut |src| {
            src.is_file() || !src.ends_with("target")
        })?;

        Ok(())
    }

    /// Fix the contents of the copied package's `Cargo.toml`: name altered to match
    /// `package.dst_name` and local relative-path-based dependencies are updated to account for the
    /// copied package's new location.  Assumes that all copied files exist (but may not contain
    /// up-to-date information).
    fn update_package(&self, package: &CutPackage) -> Result<()> {
        let path = package.dst_path.join("Cargo.toml");
        let mut toml = fs::read_to_string(&path)?.parse::<Document>()?;

        // Update the package name
        toml["package"]["name"] = toml_edit::value(&package.dst_name);

        // Fix-up references to any kind of dependency (dependencies, dev-dependencies,
        // build-dependencies, target-specific dependencies).
        self.update_dependencies(&package.src_path, &package.dst_path, toml.as_table_mut())?;

        if let Some(targets) = toml.get_mut("target").and_then(Item::as_table_like_mut) {
            for (_, target) in targets.iter_mut() {
                if let Some(target) = target.as_table_like_mut() {
                    self.update_dependencies(&package.src_path, &package.dst_path, target)?;
                };
            }
        };

        fs::write(&path, toml.to_string())?;
        Ok(())
    }

    /// Find all dependency tables in `table`, part of a manifest at `dst_path/Cargo.toml`
    /// (originally at `src_path/Cargo.toml`), and fix (relative) paths to account for the change in
    /// the package's location.
    fn update_dependencies(
        &self,
        src_path: impl AsRef<Path>,
        dst_path: impl AsRef<Path>,
        table: &mut dyn toml_edit::TableLike,
    ) -> Result<()> {
        for field in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(deps) = table.get_mut(field).and_then(Item::as_table_like_mut) else {
                continue;
            };

            for (dep_name, dep) in deps.iter_mut() {
                self.update_dependency(&src_path, &dst_path, dep_name, dep)?
            }
        }

        Ok(())
    }

    /// Update an individual dependency from a copied package manifest.  Only local path-based
    /// dependencies are updated:
    ///
    ///     Dep = { path = "..." }
    ///
    /// If `Dep` is another package to be copied as part of this plan, the path is updated to the
    /// location it is copied to.  Otherwise, its location (a relative path) is updated to account
    /// for the fact that the copied package is at a new location.
    fn update_dependency(
        &self,
        src_path: impl AsRef<Path>,
        dst_path: impl AsRef<Path>,
        dep_name: toml_edit::KeyMut,
        dep: &mut Item,
    ) -> Result<()> {
        let Some(dep) = dep.as_table_like_mut() else {
            return Ok(());
        };

        // If the dep has an explicit package name, use that as the key for finding package
        // information, rather than the field name of the dep.
        let dep_pkg = self.packages.get(
            dep.get("package")
                .and_then(Item::as_str)
                .unwrap_or_else(|| dep_name.get()),
        );

        // Only path-based dependencies need to be updated.
        let Some(path) = dep.get_mut("path") else {
            return Ok(());
        };

        if let Some(dep_pkg) = dep_pkg {
            // Dependency is for a package that was cut, redirect to the cut package.
            *path = toml_edit::value(path_to_toml_value(dst_path, &dep_pkg.dst_path)?);
            if dep_name.get() != dep_pkg.dst_name {
                dep.insert("package", toml_edit::value(&dep_pkg.dst_name));
            }
        } else if let Some(rel_dep_path) = path.as_str() {
            // Dependency is for an existing (non-cut) local package, fix up its (relative) path to
            // now be relative to its cut location.
            let dep_path = src_path.as_ref().join(rel_dep_path);
            *path = toml_edit::value(path_to_toml_value(dst_path, dep_path)?);
        }

        Ok(())
    }

    /// Add entries to the `members` and `exclude` arrays in the root manifest's `workspace` table.
    fn update_workspace(&self) -> Result<()> {
        let path = self.root.join("Cargo.toml");
        if !path.exists() {
            bail!(Error::NoWorkspace(path));
        }

        let mut toml = fs::read_to_string(&path)?.parse::<Document>()?;
        for package in self.packages.values() {
            match package.ws_state {
                WorkspaceState::Unknown => {
                    continue;
                }

                WorkspaceState::Member => {
                    // This assumes that there is a "workspace.members" section, which is a fair
                    // assumption in our repo.
                    let Some(members) = toml["workspace"]["members"].as_array_mut() else {
                        bail!(Error::NotAStringArray("members"));
                    };

                    let pkg_path = path_to_toml_value(&self.root, &package.dst_path)?;
                    members.push(pkg_path);
                }

                WorkspaceState::Exclude => {
                    // This assumes that there is a "workspace.exclude" section, which is a fair
                    // assumption in our repo.
                    let Some(exclude) = toml["workspace"]["exclude"].as_array_mut() else {
                        bail!(Error::NotAStringArray("exclude"));
                    };

                    let pkg_path = path_to_toml_value(&self.root, &package.dst_path)?;
                    exclude.push(pkg_path);
                }
            };
        }

        if let Some(members) = toml
            .get_mut("workspace")
            .and_then(|w| w.get_mut("members"))
            .and_then(|m| m.as_array_mut())
        {
            format_array_of_strings("members", members)?
        }

        if let Some(exclude) = toml
            .get_mut("workspace")
            .and_then(|w| w.get_mut("exclude"))
            .and_then(|m| m.as_array_mut())
        {
            format_array_of_strings("exclude", exclude)?
        }

        fs::write(&path, toml.to_string())?;
        Ok(())
    }

    /// Attempt to clean-up the partial results of executing a plan, by deleting the directories
    /// that the plan would have created.  Swallows and prints errors to make sure as much clean-up
    /// as possible is done -- this function is typically called when some other error has occurred,
    /// so it's unclear what it's starting state would be.
    fn rollback(&self) {
        for dir in &self.directories {
            if let Err(e) = fs::remove_dir_all(dir) {
                eprintln!("Rollback Error deleting {}: {e}", dir.display());
            }
        }
    }
}

impl Workspace {
    /// Read `members` and `exclude` from the `workspace` section of the `Cargo.toml` file in
    /// directory `root`.  Fails if there isn't a manifest, it doesn't contain a `workspace`
    /// section, or the relevant fields are not formatted as expected.
    fn read<P: AsRef<Path>>(root: P) -> Result<Self> {
        let path = root.as_ref().join("Cargo.toml");
        if !path.exists() {
            bail!(Error::NoWorkspace(path));
        }

        let toml = toml::de::from_str::<Value>(&fs::read_to_string(&path)?)?;
        let Some(workspace) = toml.get("workspace") else {
            bail!(Error::NoWorkspace(path));
        };

        let members = toml_path_array_to_set(root.as_ref(), workspace, "members")
            .context("Failed to read workspace.members")?;
        let exclude = toml_path_array_to_set(root.as_ref(), workspace, "exclude")
            .context("Failed to read workspace.exclude")?;

        Ok(Self { members, exclude })
    }

    /// Determine the state of the path insofar as whether it is a direct member or exclude of this
    /// `Workspace`.
    fn state<P: AsRef<Path>>(&self, path: P) -> Result<WorkspaceState> {
        let path = path.as_ref();
        match (self.members.contains(path), self.exclude.contains(path)) {
            (true, true) => bail!(Error::WorkspaceConflict(path.to_path_buf())),

            (true, false) => Ok(WorkspaceState::Member),
            (false, true) => Ok(WorkspaceState::Exclude),
            (false, false) => Ok(WorkspaceState::Unknown),
        }
    }
}

impl fmt::Display for CutPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Copying packages in: {}", self.root.display())?;

        fn write_package(
            root: &Path,
            name: &str,
            pkg: &CutPackage,
            f: &mut fmt::Formatter<'_>,
        ) -> fmt::Result {
            let dst_path = pkg.dst_path.strip_prefix(root).unwrap_or(&pkg.dst_path);

            let src_path = pkg.src_path.strip_prefix(root).unwrap_or(&pkg.src_path);

            writeln!(f, " - to:   {}", pkg.dst_name)?;
            writeln!(f, "         {}", dst_path.display())?;
            writeln!(f, "   from: {name}")?;
            writeln!(f, "         {}", src_path.display())?;
            Ok(())
        }

        writeln!(f)?;
        writeln!(f, "new [workspace] members:")?;
        for (name, package) in &self.packages {
            if package.ws_state == WorkspaceState::Member {
                write_package(&self.root, name, package, f)?
            }
        }

        writeln!(f)?;
        writeln!(f, "new [workspace] excludes:")?;
        for (name, package) in &self.packages {
            if package.ws_state == WorkspaceState::Exclude {
                write_package(&self.root, name, package, f)?
            }
        }

        writeln!(f)?;
        writeln!(f, "other packages:")?;
        for (name, package) in &self.packages {
            if package.ws_state == WorkspaceState::Unknown {
                write_package(&self.root, name, package, f)?
            }
        }

        Ok(())
    }
}

/// Find the root of the git repository containing `cwd`, if it exists, return `None` otherwise.
/// This function only searches prefixes of the provided path for the git repo, so if the path is
/// given as a relative path within the repository, the root will not be found.
fn discover_root(mut cwd: PathBuf) -> Option<PathBuf> {
    cwd.extend(["_", ".git"]);
    while {
        cwd.pop();
        cwd.pop()
    } {
        cwd.push(".git");
        if cwd.is_dir() {
            cwd.pop();
            return Some(cwd);
        }
    }

    None
}

/// Read `[field]` from `table`, as an array of strings, and interpret as a set of paths,
/// canonicalized relative to a `root` path.
///
/// Fails if the field does not exist, does not consist of all strings, or if a path fails to
/// canonicalize.
fn toml_path_array_to_set<P: AsRef<Path>>(
    root: P,
    table: &Value,
    field: &'static str,
) -> Result<HashSet<PathBuf>> {
    let mut set = HashSet::new();

    let Some(array) = table.get(field) else {
        return Ok(set);
    };
    let Some(array) = array.as_array() else {
        bail!(Error::NotAStringArray(field))
    };

    for val in array {
        let Some(path) = val.as_str() else {
            bail!(Error::NotAStringArray(field));
        };

        set.insert(
            fs::canonicalize(root.as_ref().join(path))
                .with_context(|| format!("Canonicalizing path '{path}'"))?,
        );
    }

    Ok(set)
}

/// Represent `path` as a TOML value, by first describing it as a relative path (relative to
/// `root`), and then converting it to a String.  Fails if either `root` or `path` are not real
/// paths (cannot be canonicalized), or the resulting relative path cannot be represented as a
/// String.
fn path_to_toml_value<P, Q>(root: P, path: Q) -> Result<toml_edit::Value>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let path = path_relative_to(root, path)?;
    let Some(repr) = path.to_str() else {
        bail!(Error::PathToTomlStr(path));
    };

    Ok(repr.into())
}

/// Format a TOML array of strings: Splits elements over multiple lines, indents them, sorts them,
/// and adds a trailing comma.
fn format_array_of_strings(field: &'static str, array: &mut toml_edit::Array) -> Result<()> {
    let mut strs = BTreeSet::new();
    for item in &*array {
        let Some(s) = item.as_str() else {
            bail!(Error::NotAStringArray(field));
        };

        strs.insert(s.to_owned());
    }

    array.set_trailing_comma(true);
    array.set_trailing("\n");
    array.clear();

    for s in strs {
        array.push_formatted(toml_edit::Value::from(s).decorated("\n    ", ""));
    }

    Ok(())
}

fn package_name<P: AsRef<Path>>(path: P) -> Result<Option<String>> {
    if !path.as_ref().is_file() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)?;
    let toml = toml::de::from_str::<Value>(&content)?;

    let Some(package) = toml.get("package") else {
        return Ok(None);
    };

    let Some(name) = package.get("name") else {
        return Ok(None);
    };

    Ok(name.as_str().map(str::to_string))
}

#[cfg(test)]
mod tests {
    use crate::args::Directory;

    use super::*;

    use expect_test::expect;
    use std::fmt;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_discover_root() {
        let cut = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        let Some(root) = discover_root(cut.clone()) else {
            panic!("Failed to discover root from: {}", cut.display());
        };

        assert!(cut.starts_with(root));
    }

    #[test]
    fn test_discover_root_idempotence() {
        let cut = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        let Some(root) = discover_root(cut.clone()) else {
            panic!("Failed to discover root from: {}", cut.display());
        };

        let Some(root_again) = discover_root(root.clone()) else {
            panic!("Failed to discover root from itself: {}", root.display());
        };

        assert_eq!(root, root_again);
    }

    #[test]
    fn test_discover_root_non_existent() {
        let tmp = tempdir().unwrap();
        assert_eq!(None, discover_root(tmp.path().to_owned()));
    }

    #[test]
    fn test_workspace_read() {
        let cut = fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
        let root = discover_root(cut.clone()).unwrap();

        let sui_execution = root.join("sui-execution");
        let move_vm_types = root.join("external-crates/move/crates/move-vm-types");

        let ws = Workspace::read(&root).unwrap();

        // This crate is a member of the workspace
        assert!(ws.members.contains(&cut));

        // Other examples
        assert!(ws.members.contains(&sui_execution));
        assert!(ws.exclude.contains(&move_vm_types));
    }

    #[test]
    fn test_no_workspace() {
        let err = Workspace::read(env!("CARGO_MANIFEST_DIR")).unwrap_err();
        expect!["No [workspace] found at $PATH/sui-execution/cut/Cargo.toml/Cargo.toml"]
            .assert_eq(&scrub_path(&format!("{:#}", err), repo_root()));
    }

    #[test]
    fn test_empty_workspace() {
        let tmp = tempdir().unwrap();
        let toml = tmp.path().join("Cargo.toml");

        fs::write(
            toml,
            r#"
              [workspace]
            "#,
        )
        .unwrap();

        let ws = Workspace::read(&tmp).unwrap();
        assert!(ws.members.is_empty());
        assert!(ws.exclude.is_empty());
    }

    #[test]
    fn test_bad_workspace_field() {
        let tmp = tempdir().unwrap();
        let toml = tmp.path().join("Cargo.toml");

        fs::write(
            toml,
            r#"
              [workspace]
              members = [1, 2, 3]
            "#,
        )
        .unwrap();

        let err = Workspace::read(&tmp).unwrap_err();
        expect!["Failed to read workspace.members: 'members' field is not an array of strings"]
            .assert_eq(&scrub_path(&format!("{:#}", err), repo_root()));
    }

    #[test]
    fn test_bad_workspace_path() {
        let tmp = tempdir().unwrap();
        let toml = tmp.path().join("Cargo.toml");

        fs::write(
            toml,
            r#"
              [workspace]
              members = ["i_dont_exist"]
            "#,
        )
        .unwrap();

        let err = Workspace::read(&tmp).unwrap_err();
        expect!["Failed to read workspace.members: Canonicalizing path 'i_dont_exist': No such file or directory (os error 2)"]
        .assert_eq(&scrub_path(&format!("{:#}", err), repo_root()));
    }

    #[test]
    fn test_cut_plan_discover() {
        let cut = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        let plan = CutPlan::discover(Args {
            dry_run: false,
            workspace_update: true,
            feature: "feature".to_string(),
            root: None,
            directories: vec![
                Directory {
                    src: cut.join("../latest"),
                    dst: cut.join("../exec-cut"),
                    suffix: Some("-latest".to_string()),
                },
                Directory {
                    src: cut.clone(),
                    dst: cut.join("../cut-cut"),
                    suffix: None,
                },
                Directory {
                    src: cut.join("../../external-crates/move/crates/move-core-types"),
                    dst: cut.join("../cut-move-core-types"),
                    suffix: None,
                },
            ],
            packages: vec![
                "move-core-types".to_string(),
                "sui-adapter-latest".to_string(),
                "sui-execution-cut".to_string(),
                "sui-verifier-latest".to_string(),
            ],
        })
        .unwrap();

        expect![[r#"
            CutPlan {
                root: "$PATH",
                directories: {
                    "$PATH/sui-execution/cut-cut",
                    "$PATH/sui-execution/cut-move-core-types",
                    "$PATH/sui-execution/exec-cut",
                },
                packages: {
                    "move-core-types": CutPackage {
                        dst_name: "move-core-types-feature",
                        src_path: "$PATH/external-crates/move/crates/move-core-types",
                        dst_path: "$PATH/sui-execution/cut-move-core-types",
                        ws_state: Exclude,
                    },
                    "sui-adapter-latest": CutPackage {
                        dst_name: "sui-adapter-feature",
                        src_path: "$PATH/sui-execution/latest/sui-adapter",
                        dst_path: "$PATH/sui-execution/exec-cut/sui-adapter",
                        ws_state: Member,
                    },
                    "sui-execution-cut": CutPackage {
                        dst_name: "sui-execution-cut-feature",
                        src_path: "$PATH/sui-execution/cut",
                        dst_path: "$PATH/sui-execution/cut-cut",
                        ws_state: Member,
                    },
                    "sui-verifier-latest": CutPackage {
                        dst_name: "sui-verifier-feature",
                        src_path: "$PATH/sui-execution/latest/sui-verifier",
                        dst_path: "$PATH/sui-execution/exec-cut/sui-verifier",
                        ws_state: Member,
                    },
                },
            }"#]]
        .assert_eq(&debug_for_test(&plan));

        expect![[r#"
            Copying packages in: $PATH

            new [workspace] members:
             - to:   sui-adapter-feature
                     sui-execution/exec-cut/sui-adapter
               from: sui-adapter-latest
                     sui-execution/latest/sui-adapter
             - to:   sui-execution-cut-feature
                     sui-execution/cut-cut
               from: sui-execution-cut
                     sui-execution/cut
             - to:   sui-verifier-feature
                     sui-execution/exec-cut/sui-verifier
               from: sui-verifier-latest
                     sui-execution/latest/sui-verifier

            new [workspace] excludes:
             - to:   move-core-types-feature
                     sui-execution/cut-move-core-types
               from: move-core-types
                     external-crates/move/crates/move-core-types

            other packages:
        "#]]
        .assert_eq(&display_for_test(&plan));
    }

    #[test]
    fn test_cut_plan_discover_new_top_level_destination() {
        let cut = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        // Create a plan where all the new packages are gathered into a single top-level destination
        // directory, and expect that the resulting plan's `directories` only contains one entry.
        let plan = CutPlan::discover(Args {
            dry_run: false,
            workspace_update: true,
            feature: "feature".to_string(),
            root: None,
            directories: vec![
                Directory {
                    src: cut.join("../latest"),
                    dst: cut.join("../feature"),
                    suffix: Some("-latest".to_string()),
                },
                Directory {
                    src: cut.clone(),
                    dst: cut.join("../feature/cut"),
                    suffix: None,
                },
                Directory {
                    src: cut.join("../../external-crates/move"),
                    dst: cut.join("../feature/move"),
                    suffix: None,
                },
            ],
            packages: vec![
                "move-core-types".to_string(),
                "sui-adapter-latest".to_string(),
                "sui-execution-cut".to_string(),
                "sui-verifier-latest".to_string(),
            ],
        })
        .unwrap();

        expect![[r#"
            CutPlan {
                root: "$PATH",
                directories: {
                    "$PATH/sui-execution/feature",
                },
                packages: {
                    "move-core-types": CutPackage {
                        dst_name: "move-core-types-feature",
                        src_path: "$PATH/external-crates/move/crates/move-core-types",
                        dst_path: "$PATH/sui-execution/feature/move/crates/move-core-types",
                        ws_state: Exclude,
                    },
                    "sui-adapter-latest": CutPackage {
                        dst_name: "sui-adapter-feature",
                        src_path: "$PATH/sui-execution/latest/sui-adapter",
                        dst_path: "$PATH/sui-execution/feature/sui-adapter",
                        ws_state: Member,
                    },
                    "sui-execution-cut": CutPackage {
                        dst_name: "sui-execution-cut-feature",
                        src_path: "$PATH/sui-execution/cut",
                        dst_path: "$PATH/sui-execution/feature/cut",
                        ws_state: Member,
                    },
                    "sui-verifier-latest": CutPackage {
                        dst_name: "sui-verifier-feature",
                        src_path: "$PATH/sui-execution/latest/sui-verifier",
                        dst_path: "$PATH/sui-execution/feature/sui-verifier",
                        ws_state: Member,
                    },
                },
            }"#]]
        .assert_eq(&debug_for_test(&plan));
    }

    #[test]
    fn test_cut_plan_workspace_conflict() {
        let tmp = tempdir().unwrap();
        fs::create_dir(tmp.path().join("foo")).unwrap();

        fs::write(
            tmp.path().join("Cargo.toml"),
            r#"
              [workspace]
              members = ["foo"]
              exclude = ["foo"]
            "#,
        )
        .unwrap();

        fs::write(
            tmp.path().join("foo/Cargo.toml"),
            r#"
              [package]
              name = "foo"
            "#,
        )
        .unwrap();

        let err = CutPlan::discover(Args {
            dry_run: false,
            workspace_update: true,
            feature: "feature".to_string(),
            root: Some(tmp.path().to_owned()),
            directories: vec![Directory {
                src: tmp.path().to_owned(),
                dst: tmp.path().join("cut"),
                suffix: None,
            }],
            packages: vec!["foo".to_string()],
        })
        .unwrap_err();

        expect!["Failed to find packages in $PATH: Failed to plan copy for $PATH/foo: Both member and exclude of [workspace]: $PATH/foo"]
        .assert_eq(&scrub_path(&format!("{:#}", err), tmp.path()));
    }

    #[test]
    fn test_cut_plan_package_name_conflict() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("foo/bar-latest")).unwrap();
        fs::create_dir_all(tmp.path().join("baz/bar")).unwrap();

        fs::write(tmp.path().join("Cargo.toml"), "[workspace]").unwrap();

        fs::write(
            tmp.path().join("foo/bar-latest/Cargo.toml"),
            r#"package.name = "bar-latest""#,
        )
        .unwrap();

        fs::write(
            tmp.path().join("baz/bar/Cargo.toml"),
            r#"package.name = "bar""#,
        )
        .unwrap();

        let err = CutPlan::discover(Args {
            dry_run: false,
            workspace_update: true,
            feature: "feature".to_string(),
            root: Some(tmp.path().to_owned()),
            directories: vec![
                Directory {
                    src: tmp.path().join("foo"),
                    dst: tmp.path().join("cut"),
                    suffix: Some("-latest".to_string()),
                },
                Directory {
                    src: tmp.path().join("baz"),
                    dst: tmp.path().join("cut"),
                    suffix: None,
                },
            ],
            packages: vec!["bar-latest".to_string(), "bar".to_string()],
        })
        .unwrap_err();

        expect!["Packages 'bar-latest' and 'bar' map to the same cut package name"]
            .assert_eq(&format!("{:#}", err));
    }

    #[test]
    fn test_cut_plan_package_path_conflict() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("foo/bar")).unwrap();
        fs::create_dir_all(tmp.path().join("baz/bar")).unwrap();

        fs::write(tmp.path().join("Cargo.toml"), "[workspace]").unwrap();

        fs::write(
            tmp.path().join("foo/bar/Cargo.toml"),
            r#"package.name = "foo-bar""#,
        )
        .unwrap();

        fs::write(
            tmp.path().join("baz/bar/Cargo.toml"),
            r#"package.name = "baz-bar""#,
        )
        .unwrap();

        let err = CutPlan::discover(Args {
            dry_run: false,
            workspace_update: true,
            feature: "feature".to_string(),
            root: Some(tmp.path().to_owned()),
            directories: vec![
                Directory {
                    src: tmp.path().join("foo"),
                    dst: tmp.path().join("cut"),
                    suffix: None,
                },
                Directory {
                    src: tmp.path().join("baz"),
                    dst: tmp.path().join("cut"),
                    suffix: None,
                },
            ],
            packages: vec!["foo-bar".to_string(), "baz-bar".to_string()],
        })
        .unwrap_err();

        expect!["Packages 'foo-bar' and 'baz-bar' map to the same cut package path"]
            .assert_eq(&format!("{:#}", err));
    }

    #[test]
    fn test_cut_plan_existing_package() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("foo/bar")).unwrap();
        fs::create_dir_all(tmp.path().join("baz/bar")).unwrap();

        fs::write(tmp.path().join("Cargo.toml"), "[workspace]").unwrap();

        fs::write(
            tmp.path().join("foo/bar/Cargo.toml"),
            r#"package.name = "foo-bar""#,
        )
        .unwrap();

        fs::write(
            tmp.path().join("baz/bar/Cargo.toml"),
            r#"package.name = "baz-bar""#,
        )
        .unwrap();

        let err = CutPlan::discover(Args {
            dry_run: false,
            workspace_update: true,
            feature: "feature".to_string(),
            root: Some(tmp.path().to_owned()),
            directories: vec![Directory {
                src: tmp.path().join("foo"),
                dst: tmp.path().join("baz"),
                suffix: None,
            }],
            packages: vec!["foo-bar".to_string()],
        })
        .unwrap_err();

        expect!["Failed to find packages in $PATH/foo: Failed to plan copy for $PATH/foo/bar: Cutting package 'foo-bar' will overwrite existing path: $PATH/baz/bar"]
        .assert_eq(&scrub_path(&format!("{:#}", err), tmp.path()));
    }

    #[test]
    fn test_cut_plan_execute_and_rollback() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_owned();

        fs::create_dir_all(root.join("crates/foo/../bar/../baz/../qux/../quy")).unwrap();

        fs::write(
            root.join("Cargo.toml"),
            [
                r#"[workspace]"#,
                r#"members = ["crates/foo"]"#,
                r#"exclude = ["#,
                r#"    "crates/bar","#,
                r#"    "crates/qux","#,
                r#"]"#,
            ]
            .join("\n"),
        )
        .unwrap();

        fs::write(
            root.join("crates/foo/Cargo.toml"),
            r#"package.name = "foo-latest""#,
        )
        .unwrap();

        fs::write(
            root.join("crates/bar/Cargo.toml"),
            [
                r#"[package]"#,
                r#"name = "bar""#,
                r#""#,
                r#"[dependencies]"#,
                r#"foo = { path = "../foo", package = "foo-latest" }"#,
                r#""#,
                r#"[dev-dependencies]"#,
                r#"baz = { path = "../baz" }"#,
                r#"quy = { path = "../quy" }"#,
            ]
            .join("\n"),
        )
        .unwrap();

        fs::write(
            root.join("crates/baz/Cargo.toml"),
            [
                r#"[package]"#,
                r#"name = "baz""#,
                r#""#,
                r#"[dependencies]"#,
                r#"acme = "1.0.0""#,
                r#""#,
                r#"[build-dependencies]"#,
                r#"bar = { path = "../bar" }"#,
            ]
            .join("\n"),
        )
        .unwrap();

        fs::write(
            root.join("crates/qux/Cargo.toml"),
            [
                r#"[package]"#,
                r#"name = "qux""#,
                r#""#,
                r#"[target.'cfg(unix)'.dependencies]"#,
                r#"bar = { path = "../bar" }"#,
                r#""#,
                r#"[target.'cfg(target_arch = "x86_64")'.build-dependencies]"#,
                r#"foo = { path = "../foo", package = "foo-latest" }"#,
            ]
            .join("\n"),
        )
        .unwrap();

        fs::write(
            root.join("crates/quy/Cargo.toml"),
            [r#"[package]"#, r#"name = "quy""#].join("\n"),
        )
        .unwrap();

        let plan = CutPlan::discover(Args {
            dry_run: false,
            workspace_update: true,
            feature: "cut".to_string(),
            root: Some(tmp.path().to_owned()),
            directories: vec![Directory {
                src: root.join("crates"),
                dst: root.join("cut"),
                suffix: Some("-latest".to_owned()),
            }],
            packages: vec![
                "foo-latest".to_string(),
                "bar".to_string(),
                "baz".to_string(),
                "qux".to_string(),
            ],
        })
        .unwrap();

        plan.execute().unwrap();

        assert!(!root.join("cut/quy").exists());

        expect![[r#"
            [workspace]
            members = [
                "crates/foo",
                "cut/foo",
            ]
            exclude = [
                "crates/bar",
                "crates/qux",
                "cut/bar",
                "cut/qux",
            ]

            ---
            package.name = "foo-cut"

            ---
            [package]
            name = "bar-cut"

            [dependencies]
            foo = { path = "../foo", package = "foo-cut" }

            [dev-dependencies]
            baz = { path = "../baz", package = "baz-cut" }
            quy = { path = "../../crates/quy" }

            ---
            [package]
            name = "baz-cut"

            [dependencies]
            acme = "1.0.0"

            [build-dependencies]
            bar = { path = "../bar", package = "bar-cut" }

            ---
            [package]
            name = "qux-cut"

            [target.'cfg(unix)'.dependencies]
            bar = { path = "../bar", package = "bar-cut" }

            [target.'cfg(target_arch = "x86_64")'.build-dependencies]
            foo = { path = "../foo", package = "foo-cut" }
        "#]]
        .assert_eq(&read_files([
            root.join("Cargo.toml"),
            root.join("cut/foo/Cargo.toml"),
            root.join("cut/bar/Cargo.toml"),
            root.join("cut/baz/Cargo.toml"),
            root.join("cut/qux/Cargo.toml"),
        ]));

        plan.rollback();
        assert!(!root.join("cut").exists())
    }

    #[test]
    fn test_cut_plan_no_workspace_update() {
        let cut = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        let plan = CutPlan::discover(Args {
            dry_run: false,
            workspace_update: false,
            feature: "feature".to_string(),
            root: None,
            directories: vec![
                Directory {
                    src: cut.join("../latest"),
                    dst: cut.join("../exec-cut"),
                    suffix: Some("-latest".to_string()),
                },
                Directory {
                    src: cut.clone(),
                    dst: cut.join("../cut-cut"),
                    suffix: None,
                },
                Directory {
                    src: cut.join("../../external-crates/move/crates/move-core-types"),
                    dst: cut.join("../cut-move-core-types"),
                    suffix: None,
                },
            ],
            packages: vec![
                "move-core-types".to_string(),
                "sui-adapter-latest".to_string(),
                "sui-execution-cut".to_string(),
                "sui-verifier-latest".to_string(),
            ],
        })
        .unwrap();

        expect![[r#"
            CutPlan {
                root: "$PATH",
                directories: {
                    "$PATH/sui-execution/cut-cut",
                    "$PATH/sui-execution/cut-move-core-types",
                    "$PATH/sui-execution/exec-cut",
                },
                packages: {
                    "move-core-types": CutPackage {
                        dst_name: "move-core-types-feature",
                        src_path: "$PATH/external-crates/move/crates/move-core-types",
                        dst_path: "$PATH/sui-execution/cut-move-core-types",
                        ws_state: Unknown,
                    },
                    "sui-adapter-latest": CutPackage {
                        dst_name: "sui-adapter-feature",
                        src_path: "$PATH/sui-execution/latest/sui-adapter",
                        dst_path: "$PATH/sui-execution/exec-cut/sui-adapter",
                        ws_state: Unknown,
                    },
                    "sui-execution-cut": CutPackage {
                        dst_name: "sui-execution-cut-feature",
                        src_path: "$PATH/sui-execution/cut",
                        dst_path: "$PATH/sui-execution/cut-cut",
                        ws_state: Unknown,
                    },
                    "sui-verifier-latest": CutPackage {
                        dst_name: "sui-verifier-feature",
                        src_path: "$PATH/sui-execution/latest/sui-verifier",
                        dst_path: "$PATH/sui-execution/exec-cut/sui-verifier",
                        ws_state: Unknown,
                    },
                },
            }"#]]
        .assert_eq(&debug_for_test(&plan));
    }

    /// Print with pretty-printed debug formatting, with repo paths scrubbed out for consistency.
    fn debug_for_test<T: fmt::Debug>(x: &T) -> String {
        scrub_path(&format!("{x:#?}"), repo_root())
    }

    /// Print with display formatting, with repo paths scrubbed out for consistency.
    fn display_for_test<T: fmt::Display>(x: &T) -> String {
        scrub_path(&format!("{x}"), repo_root())
    }

    /// Read multiple files into one string.
    fn read_files<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>) -> String {
        let contents: Vec<_> = paths
            .into_iter()
            .map(|p| fs::read_to_string(p).unwrap())
            .collect();

        contents.join("\n---\n")
    }

    fn scrub_path<P: AsRef<Path>>(x: &str, p: P) -> String {
        let path0 = fs::canonicalize(&p)
            .unwrap()
            .into_os_string()
            .into_string()
            .unwrap();

        let path1 = p.as_ref().as_os_str().to_os_string().into_string().unwrap();

        x.replace(&path0, "$PATH").replace(&path1, "$PATH")
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
}
