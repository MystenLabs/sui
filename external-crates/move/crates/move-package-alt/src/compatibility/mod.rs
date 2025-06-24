pub mod legacy;
pub mod legacy_parser;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use move_core_types::account_address::AccountAddress;
use regex::Regex;
use tokio::spawn;

use crate::package::PackageName;
use crate::package::layout::SourcePackageLayout;

pub type LegacyAddressDeclarations = BTreeMap<String, Option<AccountAddress>>;
pub type LegacyDevAddressDeclarations = BTreeMap<String, AccountAddress>;
pub type LegacyVersion = (u64, u64, u64);
pub type LegacySubstitution = BTreeMap<String, LegacySubstOrRename>;

#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct LegacyBuildInfo {
    pub language_version: Option<LegacyVersion>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum LegacySubstOrRename {
    RenameFrom(String),
    Assign(AccountAddress),
}

/// Normalize the representation of `path` by eliminating redundant `.` components and applying `..`
/// component.  Does not access the filesystem (e.g. to resolve symlinks or test for file
/// existence), unlike `std::fs::canonicalize`.
///
/// Fails if the normalized path attempts to access the parent of a root directory or volume prefix,
/// or is prefixed by accesses to parent directories when `allow_cwd_parent` is false.
///
/// Returns the normalized path on success.
pub fn normalize_path(path: impl AsRef<Path>, allow_cwd_parent: bool) -> Result<PathBuf> {
    use std::path::Component::*;

    let mut stack = Vec::new();
    for component in path.as_ref().components() {
        match component {
            // Components that contribute to the path as-is.
            verbatim @ (Prefix(_) | RootDir | Normal(_)) => stack.push(verbatim),

            // Equivalent of a `.` path component -- can be ignored.
            CurDir => { /* nop */ }

            // Going up in the directory hierarchy, which may fail if that's not possible.
            ParentDir => match stack.last() {
                None | Some(ParentDir) => {
                    stack.push(ParentDir);
                }

                Some(Normal(_)) => {
                    stack.pop();
                }

                Some(CurDir) => {
                    unreachable!("Component::CurDir never added to the stack");
                }

                Some(RootDir | Prefix(_)) => bail!(
                    "Invalid path accessing parent of root directory: {}",
                    path.as_ref().to_string_lossy(),
                ),
            },
        }
    }

    let normalized: PathBuf = stack.iter().collect();
    if !allow_cwd_parent && stack.first() == Some(&ParentDir) {
        bail!(
            "Path cannot access parent of current directory: {}",
            normalized.to_string_lossy()
        );
    }

    Ok(normalized)
}

/// The regex to detect `module <name>::<module_name>` on its different forms.
const MODULE_REGEX: &str = r"\bmodule\s+([a-zA-Z_][\w]*)::([a-zA-Z_][\w]*)";

/// This is a naive way to detect all module names that are part of the source code
/// for a given package.
///
/// This helps us when we fail to detect any module name with 0x0 in the Manifest file,
pub(crate) fn find_module_name_for_package(root_path: &Path) -> Result<PackageName> {
    let mut files = Vec::new();
    find_files(
        &mut files,
        &root_path.join(SourcePackageLayout::Sources.path()),
        "move",
        5,
    );

    // TODO: Should we also look into tests folder, in case we're dealing with a `tests`-only package?
    let mut names = HashSet::new();

    for file in files {
        let file_contents = fs::read_to_string(file)?;
        let module_names = parse_module_names(&file_contents)?;

        if module_names.len() > 0 {
            names.extend(module_names);
        }
    }

    if names.len() > 1 {
        bail!("Multiple module names found in the package.");
    }

    let Some(name) = names.iter().next() else {
        bail!("No module names found in the package.");
    };

    Ok(PackageName::new(name.as_str())?)
}

/// Find all files matching the extension in a given path.
fn find_files(files: &mut Vec<PathBuf>, dir: &Path, extension: &str, max_depth: usize) {
    if max_depth == 0 {
        return;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext == extension {
                            files.push(path);
                        }
                    }
                } else if metadata.is_dir() {
                    find_files(files, &path, extension, max_depth - 1);
                }
            }
        }
    }
}

// TODO: Conside supporting the legacy `address { module {} }` format.
fn parse_module_names(contents: &str) -> Result<HashSet<String>> {
    let mut set = HashSet::new();
    // This matches `module a::b {}`, and `module a::b;` cases.
    // In both cases, the match is the 2nd group (so `match.get(1)`)
    let regex = Regex::new(MODULE_REGEX).unwrap();

    for cap in regex.captures_iter(&contents) {
        set.insert(cap[1].to_string());
    }

    Ok(set)
}

#[cfg(test)]
mod tests {
    use std::hash::Hash;

    use super::*;

    fn set(vec: Vec<&str>) -> HashSet<String> {
        vec.into_iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_find_module_name_for_package() {
        let test_cases = vec![
            ("module a::a {}", set(vec!["a"])),
            ("module a::b {}", set(vec!["a"])),
            (
                r"module a::a
            {
            }",
                set(vec!["a"]),
            ),
            (
                r"
            module a::a{}
            module a::a_t {}
            ",
                set(vec!["a"]),
            ),
            (
                r"
            module a::a{}
            module b::a {}
            ",
                set(vec!["a", "b"]),
            ),
        ];

        for (input, expected) in test_cases {
            let module_names = parse_module_names(input).unwrap();
            assert_eq!(module_names, expected);
        }
    }
}
