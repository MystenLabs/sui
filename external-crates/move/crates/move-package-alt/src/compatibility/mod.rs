pub mod legacy;
pub mod legacy_parser;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use move_core_types::account_address::AccountAddress;
use regex::Regex;

use crate::package::layout::SourcePackageLayout;
use crate::package::paths::PackagePath;
use crate::schema::PackageName;

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

/// The regex to detect `module <name>::<module_name>` on its different forms.
const MODULE_REGEX: &str = r"\bmodule\s+([a-zA-Z_][\w]*)::([a-zA-Z_][\w]*)";

/// This is a naive way to detect all module names that are part of the source code
/// for a given package.
///
/// This helps us when we fail to detect any module name with 0x0 in the Manifest file,
pub(crate) fn find_module_name_for_package(path: &PackagePath) -> Result<PackageName> {
    let mut files = Vec::new();
    find_files(
        &mut files,
        &path.path().join(SourcePackageLayout::Sources.path()),
        "move",
        5,
    );

    // TODO: Should we also look into tests folder, in case we're dealing with a `tests`-only package?
    let mut names = HashSet::new();

    for file in files {
        let file_contents = fs::read_to_string(file)?;
        let module_names = parse_module_names(&file_contents)?;

        if !module_names.is_empty() {
            names.extend(module_names);
        }
    }

    if names.len() > 1 {
        bail!("Multiple module names found in the package.");
    }

    let Some(name) = names.iter().next() else {
        bail!("No module names found in the package.");
    };

    PackageName::new(name.as_str())
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

// Consider supporting the legacy `address { module {} }` format.
fn parse_module_names(contents: &str) -> Result<HashSet<String>> {
    let clean = strip_comments(contents);
    let mut set = HashSet::new();
    // This matches `module a::b {}`, and `module a::b;` cases.
    // In both cases, the match is the 2nd group (so `match.get(1)`)
    let regex = Regex::new(MODULE_REGEX).unwrap();

    for cap in regex.captures_iter(&clean) {
        set.insert(cap[1].to_string());
    }

    Ok(set)
}

/// Returns a copy of `source` with all the comments removed.
fn strip_comments(source: &str) -> String {
    let mut result = String::new();
    let mut in_block_doc = false;

    for line in source.lines() {
        let mut line_cleaned = line.to_string();

        // Catch the `///` case.
        if let Some(start) = line_cleaned.find("///") {
            line_cleaned.replace_range(start.., "");
        }

        // Catch the `//` case.
        if let Some(start) = line_cleaned.find("//") {
            line_cleaned.replace_range(start.., "");
        }

        if in_block_doc {
            if let Some(end) = line_cleaned.find("*/") {
                line_cleaned.replace_range(..=end + 1, ""); // remove up to and including */
                in_block_doc = false;
            } else {
                continue; // inside block doc, skip entire line
            }
        }

        // Remove any inline doc block comments (multiple if present)
        while let Some(start) = line_cleaned.find("/*") {
            if let Some(end) = line_cleaned[start..].find("*/") {
                line_cleaned.replace_range(start..start + end + 2, "");
            } else {
                // Start of multiline doc block; remove to end of line and set flag
                line_cleaned.replace_range(start.., "");
                in_block_doc = true;
                break;
            }
        }

        result.push_str(&line_cleaned);
    }

    result
}

#[cfg(test)]
mod tests {
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
            (
                r" module b {}
            ",
                set(vec![]),
            ),
            (
                r"
                /// module yy::ff {
                ///     module a::b {}
                /// }
                module works::perfectly {}
            ",
                set(vec!["works"]),
            ),
            (
                r"
                /* module yy::ff {
                    module a::b {}
                }
                */
                /// module aa::bb {
                /// 
                /// }
                module works::perfectly {}
            ",
                set(vec!["works"]),
            ),
            (
                r"
                /* module aa::bb {} */
                /* module ee::dd {} */
                module a::b {}
                ",
                set(vec!["a"]),
            ),
            (
                r"
                /*
                    multi-line comments
                    module a::b {} */
                    module works::perfectly {}
                ",
                set(vec!["works"]),
            ),
            (
                r"
                /* module aa::bb {} */ module a::b {}
                ",
                set(vec!["a"]),
            ),
            (
                r"
                /* module aa::bb {} */ /* module ee::dd {} */ module a::b {}
                ",
                set(vec!["a"]),
            ),
            (
                r"
                   module a::b {} // module bb::aa {}
                ",
                set(vec!["a"]),
            ),
            (
                r"
                module a::/* this is odd but 
                it works */b {} // module bb::aa {}
                ",
                set(vec!["a"]),
            ),
        ];

        for (input, expected) in test_cases {
            let module_names = parse_module_names(input).unwrap();
            assert_eq!(module_names, expected);
        }
    }
}
