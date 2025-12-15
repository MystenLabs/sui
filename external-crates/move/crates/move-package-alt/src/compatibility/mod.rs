pub mod legacy;
pub mod legacy_lockfile;
pub mod legacy_parser;

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use move_core_types::account_address::{AccountAddress, AccountAddressParseError};
use once_cell::sync::Lazy;
use regex::Regex;
use tracing::debug;

use crate::compatibility::legacy_parser::{LegacyPackageMetadata, parse_package_info};
use crate::package::layout::SourcePackageLayout;
use crate::package::paths::PackagePath;
use crate::schema::PackageName;
use toml::value::Value as TV;

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

// Compile regex once at program startup
#[cfg(not(msim))]
fn get_module_regex() -> &'static Regex {
    static MODULE_REGEX_COMPILED: Lazy<Regex> = Lazy::new(|| Regex::new(MODULE_REGEX).unwrap());
    &MODULE_REGEX_COMPILED
}

// In simtests we need to use a thread local to avoid breaking determinism.
#[cfg(msim)]
fn get_module_regex() -> Regex {
    thread_local! {
        static MODULE_REGEX_COMPILED: Lazy<Regex> = Lazy::new(|| Regex::new(MODULE_REGEX).unwrap());
    }

    MODULE_REGEX_COMPILED.with(|val| (*val).clone())
}

/// This is a naive way to detect all module names that are part of the source code
/// for a given package.
///
/// This helps us when we fail to detect any module name with 0x0 in the Manifest file,
pub(crate) fn find_module_name_for_package(path: &PackagePath) -> Result<Option<PackageName>> {
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

    debug!(
        "Parsed source for finding module names for package. Names are: {:?}",
        names
    );

    if names.len() > 1 {
        return Ok(None);
    }

    let Some(name) = names.iter().next() else {
        return Ok(None);
    };

    Ok(Some(PackageName::new(name.as_str())?))
}

// Safely parses address for both the 0x and non prefixed hex format.
fn parse_address_literal(address_str: &str) -> Result<AccountAddress, AccountAddressParseError> {
    if !address_str.starts_with("0x") {
        return AccountAddress::from_hex(address_str);
    }
    AccountAddress::from_hex_literal(address_str)
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
                    if let Some(ext) = path.extension()
                        && ext == extension
                    {
                        files.push(path);
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

    // This matches `module a::b {}`, and `module a::b;` cases.
    // In both cases, the match is the 2nd group (so `match.get(1)`)
    Ok(get_module_regex()
        .captures_iter(&clean)
        .filter_map(|cap| {
            let name = &cap[1];
            (!is_address_like(name)).then(|| name.to_string())
        })
        .collect())
}

fn is_address_like(name: &str) -> bool {
    (name.starts_with("0x") || name.starts_with("0X")) && AccountAddress::from_hex(name).is_ok()
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

/// Return legacy package metadata; this is needed for tests in sui side
pub fn parse_legacy_package_info(
    package_path: &Path,
) -> Result<LegacyPackageMetadata, anyhow::Error> {
    let manifest_string = std::fs::read_to_string(package_path.join("Move.toml"))?;
    let tv =
        toml::from_str::<TV>(&manifest_string).context("Unable to parse Move package manifest")?;

    match tv {
        TV::Table(mut table) => {
            let metadata = table
                .remove("package")
                .map(parse_package_info)
                .transpose()
                .context("Error parsing '[package]' section of manifest")?
                .unwrap();
            Ok(metadata)
        }
        _ => bail!("Expected a table from the manifest file"),
    }
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
            (
                r"
                module 0x0::a {}
                module 0X0::b {}
                ",
                set(vec![]),
            ),
            (
                r"
                module 0x0::a {}
                module a::b {}
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
