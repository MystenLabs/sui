pub mod legacy_manifest_parser;
pub mod legacy;
mod legacy_manifest;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use regex::Regex;
use tokio::spawn;

use crate::package::layout::SourcePackageLayout;
use crate::package::PackageName;

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

    eprintln!("{:?}", contents);

    for cap in regex.captures_iter(&contents) {
        eprintln!("{:?}", cap);
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
