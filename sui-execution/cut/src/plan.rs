// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use toml::value::Value;

use crate::args::Args;

/// Description of where packages should be copied to, what their new names should be, and whether
/// they should be added to the `workspace` `members` or `exclude` fields.
#[derive(Debug)]
pub(crate) struct CutPlan(BTreeMap<String, CutPackage>);

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
pub(crate) enum PlanError {
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

    #[error("Expected '{0}' field to be an array of strings")]
    NotAStringArray(&'static str),

    #[error("TOML Parsing Error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("IO Error: {0}")]
    IO(#[from] io::Error),
}

type PlanResult<T> = Result<T, PlanError>;

impl CutPlan {
    /// Scan `args.directories` looking for `args.packages` to produce a new plan.
    pub(crate) fn discover(args: Args) -> PlanResult<Self> {
        let cwd = env::current_dir()?;

        let Some(root) = args.root.or_else(|| discover_root(cwd)) else {
            return Err(PlanError::NoRoot);
        };

        struct Walker {
            feature: String,
            ws: Workspace,
            planned_packages: BTreeMap<String, CutPackage>,
            pending_packages: HashSet<String>,
        }

        impl Walker {
            fn walk(&mut self, src: &Path, dst: &Path, suffix: &Option<String>) -> PlanResult<()> {
                self.try_insert_package(src, dst, suffix)?;

                for entry in fs::read_dir(src)? {
                    let entry = entry?;
                    if entry.file_type()?.is_dir() {
                        self.walk(
                            &src.join(entry.file_name()),
                            &dst.join(entry.file_name()),
                            suffix,
                        )?;
                    }
                }

                Ok(())
            }

            fn try_insert_package(
                &mut self,
                src: &Path,
                dst: &Path,
                suffix: &Option<String>,
            ) -> PlanResult<()> {
                let toml = src.join("Cargo.toml");

                let Some(pkg_name) = package_name(toml)? else {
                    return Ok(())
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

                self.planned_packages.insert(
                    pkg_name,
                    CutPackage {
                        dst_name: dst_name.clone(),
                        src_path: src.to_path_buf(),
                        dst_path: dst.to_path_buf(),
                        ws_state: self.ws.state(src)?,
                    },
                );

                Ok(())
            }

            fn finish(self) -> BTreeMap<String, CutPackage> {
                self.planned_packages
            }
        }

        let mut walker = Walker {
            feature: args.feature,
            ws: Workspace::read(root)?,
            planned_packages: BTreeMap::new(),
            pending_packages: args.packages.into_iter().collect(),
        };

        for dir in args.directories {
            walker.walk(&fs::canonicalize(dir.src)?, &dir.dst, &dir.suffix)?;
        }

        // Emit warnings for packages that were not found
        for pending in &walker.pending_packages {
            eprintln!("WARNING: Package '{pending}' not found during scan.");
        }

        let packages = walker.finish();

        //  Check for conflicts in the resulting plan
        let mut rev_name = HashMap::new();
        let mut rev_path = HashMap::new();

        for (name, pkg) in &packages {
            if let Some(prev) = rev_name.insert(pkg.dst_name.clone(), name.clone()) {
                return Err(PlanError::PackageConflictName(name.clone(), prev));
            }

            if let Some(prev) = rev_path.insert(pkg.dst_path.clone(), name.clone()) {
                return Err(PlanError::PackageConflictPath(name.clone(), prev));
            }
        }

        Ok(Self(packages))
    }
}

impl Workspace {
    /// Read `members` and `exclude` from the `workspace` section of the `Cargo.toml` file in
    /// directory `root`.  Fails if there isn't a manifest, it doesn't contain a `workspace`
    /// section, or the relevant fields are not formatted as expected.
    fn read<P: AsRef<Path>>(root: P) -> PlanResult<Self> {
        let path = root.as_ref().join("Cargo.toml");
        if !path.exists() {
            return Err(PlanError::NoWorkspace(path));
        }

        let toml = toml::de::from_str::<Value>(&fs::read_to_string(&path)?)?;
        let Some(workspace) = toml.get("workspace") else {
            return Err(PlanError::NoWorkspace(path));
        };

        let members = toml_path_array_to_set(root.as_ref(), workspace, "members")?;
        let exclude = toml_path_array_to_set(root.as_ref(), workspace, "exclude")?;

        Ok(Self { members, exclude })
    }

    /// Determine the state of the path insofar as whether it is a direct member or exclude of this
    /// `Workspace`.
    fn state<P: AsRef<Path>>(&self, path: P) -> PlanResult<WorkspaceState> {
        let path = path.as_ref();
        match (self.members.contains(path), self.exclude.contains(path)) {
            (true, true) => Err(PlanError::WorkspaceConflict(path.to_path_buf())),

            (true, false) => Ok(WorkspaceState::Member),
            (false, true) => Ok(WorkspaceState::Exclude),
            (false, false) => Ok(WorkspaceState::Unknown),
        }
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
) -> PlanResult<HashSet<PathBuf>> {
    let mut set = HashSet::new();

    let Some(array) = table.get(field) else { return Ok(set) };
    let Some(array) = array.as_array() else {
        return Err(PlanError::NotAStringArray(field))
    };

    for val in array {
        let Some(path) = val.as_str() else {
            return Err(PlanError::NotAStringArray(field));
        };

        set.insert(fs::canonicalize(root.as_ref().join(path))?);
    }

    Ok(set)
}

fn package_name<P: AsRef<Path>>(path: P) -> PlanResult<Option<String>> {
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
        let move_vm_types = root.join("external-crates/move/move-vm/types");

        let ws = Workspace::read(&root).unwrap();

        // This crate is a member of the workspace
        assert!(ws.members.contains(&cut));

        // Other examples
        assert!(ws.members.contains(&sui_execution));
        assert!(ws.exclude.contains(&move_vm_types));
    }

    #[test]
    fn test_no_workspace() {
        expect!["No [workspace] found at $PATH/sui-execution/cut/Cargo.toml/Cargo.toml"].assert_eq(
            &display_for_test(&Workspace::read(env!("CARGO_MANIFEST_DIR")).unwrap_err()),
        );
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

        expect!["Expected 'members' field to be an array of strings"]
            .assert_eq(&display_for_test(&Workspace::read(&tmp).unwrap_err()));
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

        expect!["IO Error: No such file or directory (os error 2)"]
            .assert_eq(&display_for_test(&Workspace::read(&tmp).unwrap_err()));
    }

    #[test]
    fn test_cut_plan_discover() {
        let cut = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        let plan = CutPlan::discover(Args {
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
                    src: cut.join("../../external-crates/move/move-core"),
                    dst: cut.join("../cut-move-core"),
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
            CutPlan(
                {
                    "move-core-types": CutPackage {
                        dst_name: "move-core-types-feature",
                        src_path: "$PATH/external-crates/move/move-core/types",
                        dst_path: "$PATH/sui-execution/cut/../cut-move-core/types",
                        ws_state: Exclude,
                    },
                    "sui-adapter-latest": CutPackage {
                        dst_name: "sui-adapter-feature",
                        src_path: "$PATH/sui-execution/latest/sui-adapter",
                        dst_path: "$PATH/sui-execution/cut/../exec-cut/sui-adapter",
                        ws_state: Member,
                    },
                    "sui-execution-cut": CutPackage {
                        dst_name: "sui-execution-cut-feature",
                        src_path: "$PATH/sui-execution/cut",
                        dst_path: "$PATH/sui-execution/cut/../cut-cut",
                        ws_state: Member,
                    },
                    "sui-verifier-latest": CutPackage {
                        dst_name: "sui-verifier-feature",
                        src_path: "$PATH/sui-execution/latest/sui-verifier",
                        dst_path: "$PATH/sui-execution/cut/../exec-cut/sui-verifier",
                        ws_state: Member,
                    },
                },
            )"#]]
        .assert_eq(&debug_for_test(&plan));
    }

    #[test]
    fn test_cut_plan_worksplace_conflict() {
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

        expect!["Both member and exclude of [workspace]: $PATH/foo"]
            .assert_eq(&scrub_path(&format!("{}", err), tmp.path()));
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
            .assert_eq(&scrub_path(&format!("{}", err), tmp.path()));
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
            .assert_eq(&scrub_path(&format!("{}", err), tmp.path()));
    }

    /// Print with pretty-printed debug formatting, with repo paths scrubbed out for consistency.
    fn debug_for_test<T: fmt::Debug>(x: &T) -> String {
        scrub_path(&format!("{x:#?}"), repo_root())
    }

    /// Display with repo paths scrubbed out for consistency.
    fn display_for_test<T: fmt::Display>(x: &T) -> String {
        scrub_path(&format!("{x}"), repo_root())
    }

    fn scrub_path<P: AsRef<Path>>(x: &str, p: P) -> String {
        let canonical = fs::canonicalize(p).unwrap();
        let path = canonical.into_os_string().into_string().unwrap();
        x.replace(&path, "$PATH")
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
}
