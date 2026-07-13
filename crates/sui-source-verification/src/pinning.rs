// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use move_package_alt::SourcePackageLayout;

/// A dependency pinned to a revision that can move, such as a branch or a tag.
#[derive(Debug, PartialEq, Eq)]
pub struct MovingRevision {
    pub dependency: String,
    pub rev: String,
}

/// Find the dependencies of the package at `root_path` that are pinned to a moving revision.
///
/// Legacy lockfiles record the manifest's `rev` verbatim, so a dependency on a branch such as
/// `framework/mainnet` resolves to whatever that branch points at when the package is rebuilt,
/// rather than to what it pointed at when the package was published. Such a package cannot be
/// rebuilt reproducibly. Lockfiles written by the current package system always pin commit hashes.
pub fn moving_revisions(root_path: &Path) -> Vec<MovingRevision> {
    let path = root_path.join(SourcePackageLayout::Lock.path());
    let Ok(contents) = std::fs::read_to_string(path) else {
        return vec![];
    };
    moving_revisions_in(&contents)
}

/// The moving revisions recorded in the lockfile `contents`.
fn moving_revisions_in(contents: &str) -> Vec<MovingRevision> {
    let Ok(lock) = contents.parse::<toml::Value>() else {
        return vec![];
    };

    let mut found = vec![];
    collect(&lock, None, &mut found);
    found.sort_by(|a, b| (&a.dependency, &a.rev).cmp(&(&b.dependency, &b.rev)));
    found.dedup();
    found
}

/// Walk `value`, recording any table with a `source.rev` that is not a commit hash. Lockfiles have
/// labelled dependencies with `id` and, in older versions, `name`; `key` is the name the table was
/// found under, used when it carries neither.
fn collect(value: &toml::Value, key: Option<&str>, found: &mut Vec<MovingRevision>) {
    if let Some(elements) = value.as_array() {
        for element in elements {
            collect(element, key, found);
        }
        return;
    }

    let Some(table) = value.as_table() else {
        return;
    };

    if let Some(rev) = table
        .get("source")
        .and_then(|source| source.get("rev"))
        .and_then(|rev| rev.as_str())
        && !is_commit_hash(rev)
    {
        let dependency = table
            .get("id")
            .or_else(|| table.get("name"))
            .and_then(|name| name.as_str())
            .or(key)
            .unwrap_or("<unknown>");

        found.push(MovingRevision {
            dependency: dependency.to_string(),
            rev: rev.to_string(),
        });
    }

    for (name, child) in table {
        collect(child, Some(name), found);
    }
}

/// Whether `rev` looks like a git commit hash (pinned) rather than a branch or tag (moving).
///
/// Lockfiles historically recorded truncated hashes as short as ten hex digits, so any all-hex
/// string of at least seven characters (git's shortest abbreviation) is treated as a pinned commit;
/// a hex run that long is overwhelmingly a commit rather than a branch name.
fn is_commit_hash(rev: &str) -> bool {
    (7..=40).contains(&rev.len()) && rev.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A legacy lockfile records the manifest's `rev` verbatim, so branches survive into it.
    #[test]
    fn legacy_lock_reports_branches_but_not_hashes() {
        let lock = r#"
            [move]
            version = 3

            [[move.package]]
            id = "Sui"
            source = { git = "https://github.com/MystenLabs/sui.git", rev = "framework/mainnet", subdir = "x" }

            [[move.package]]
            id = "MoveStdlib"
            source = { git = "https://github.com/MystenLabs/sui.git", rev = "041c5f2bae2fe52079e44b70514333532d69f4e6" }
        "#;

        assert_eq!(
            moving_revisions_in(lock),
            vec![MovingRevision {
                dependency: "Sui".to_string(),
                rev: "framework/mainnet".to_string(),
            }]
        );
    }

    /// The current package system pins commit hashes per environment, so nothing is reported.
    #[test]
    fn pinned_lock_reports_nothing() {
        let lock = r#"
            [pinned.mainnet.token]
            source = { git = "https://github.com/MystenLabs/deepbookv3.git", rev = "5e82e2dd1ea7d47957855ddc66f835585d6fe091" }
        "#;

        assert!(moving_revisions_in(lock).is_empty());
    }

    /// Older lockfiles label dependencies with `name` rather than `id`.
    #[test]
    fn legacy_lock_uses_the_name_field() {
        let lock = r#"
            [[move.package]]
            name = "Sui"
            source = { git = "https://github.com/MystenLabs/sui.git", rev = "framework/mainnet" }
        "#;

        assert_eq!(
            moving_revisions_in(lock),
            vec![MovingRevision {
                dependency: "Sui".to_string(),
                rev: "framework/mainnet".to_string(),
            }]
        );
    }

    /// Truncated commit hashes (historically as short as ten hex digits) still pin a commit.
    #[test]
    fn truncated_hash_is_not_reported() {
        let lock = r#"
            [[move.package]]
            id = "Sui"
            source = { git = "https://github.com/MystenLabs/sui.git", rev = "041c5f2bae" }
        "#;

        assert!(moving_revisions_in(lock).is_empty());
    }

    /// A dependency with neither `id` nor `name` is named after the table it was found under.
    #[test]
    fn dependency_is_named_after_its_table_when_unlabelled() {
        let lock = r#"
            [pinned.mainnet.token]
            source = { git = "https://example.com/x.git", rev = "main" }
        "#;

        assert_eq!(
            moving_revisions_in(lock),
            vec![MovingRevision {
                dependency: "token".to_string(),
                rev: "main".to_string(),
            }]
        );
    }
}
