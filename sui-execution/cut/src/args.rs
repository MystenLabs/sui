// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{self, bail, Result};
use clap::{ArgAction, Parser};
use std::env;
use std::path::PathBuf;
use std::str::FromStr;
use thiserror::Error;

/// Tool for cutting duplicate versions of a subset of crates in a git repository.
///
/// Duplicated crate dependencies are redirected so that if a crate is duplicated with its
/// dependency, the duplicate's dependency points to the duplicated dependency.  Package names are
/// updated to avoid conflicts with their original. Duplicates respect membership or exclusion from
/// a workspace.
#[derive(Parser)]
#[command(author, version, rename_all = "kebab-case")]
pub(crate) struct Args {
    /// Name of the feature the crates are being cut for -- duplicated crate package names will be
    /// suffixed with a hyphen followed by this feature name.
    #[arg(short, long)]
    pub feature: String,

    /// Root of repository -- all source and destination paths must be within this path, and it must
    /// contain the repo's `workspace` configuration.  Defaults to the parent of the working
    /// directory that contains a .git directory.
    pub root: Option<PathBuf>,

    /// Add a directory to duplicate crates from, along with the destination to duplicate it to, and
    /// optionally a suffix to remove from package names within this directory, all separated by
    /// colons.
    ///
    /// Only crates (directories containing a `Cargo.toml` file) found under the source (first) path
    /// whose package names were supplied as a `--package` will be duplicated at the destination
    /// (second) path.  Copying will preserve the directory structure from the source directory to
    /// the destination directory.
    #[arg(short, long = "dir")]
    pub directories: Vec<Directory>,

    /// Package names to include in the cut (this must match the package name in its source
    /// location, including any suffixes)
    #[arg(short, long = "package")]
    pub packages: Vec<String>,

    /// Don't make changes to the workspace.
    #[arg(long="no-workspace-update", action=ArgAction::SetFalse)]
    pub workspace_update: bool,

    /// Don't execute the cut, just display it.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Directory {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub suffix: Option<String>,
}

#[derive(Error, Debug)]
pub(crate) enum DirectoryParseError {
    #[error("Can't parse an existing source directory from '{0}'")]
    NoSrc(String),

    #[error("Can't parse a destination directory from '{0}'")]
    NoDst(String),
}

impl FromStr for Directory {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let mut parts = s.split(':');

        let Some(src_part) = parts.next() else {
            bail!(DirectoryParseError::NoSrc(s.to_string()))
        };

        let Some(dst_part) = parts.next() else {
            bail!(DirectoryParseError::NoDst(s.to_string()))
        };

        let suffix = parts.next().map(|sfx| sfx.to_string());

        let cwd = env::current_dir()?;
        let src = cwd.join(src_part);
        let dst = cwd.join(dst_part);

        if !src.is_dir() {
            bail!(DirectoryParseError::NoSrc(src_part.to_string()));
        }

        Ok(Self { src, dst, suffix })
    }
}

#[cfg(test)]
mod tests {
    use expect_test::expect;

    use super::*;

    #[test]
    fn test_directory_parsing_everything() {
        // Source directory relative to CARGO_MANIFEST_DIR
        let dir = Directory::from_str("src:dst:suffix").unwrap();

        let cwd = env::current_dir().unwrap();
        let src = cwd.join("src");
        let dst = cwd.join("dst");

        assert_eq!(
            dir,
            Directory {
                src,
                dst,
                suffix: Some("suffix".to_string()),
            }
        )
    }

    #[test]
    fn test_directory_parsing_no_suffix() {
        // Source directory relative to CARGO_MANIFEST_DIR
        let dir = Directory::from_str("src:dst").unwrap();

        let cwd = env::current_dir().unwrap();
        let src = cwd.join("src");
        let dst = cwd.join("dst");

        assert_eq!(
            dir,
            Directory {
                src,
                dst,
                suffix: None,
            }
        )
    }

    #[test]
    fn test_directory_parsing_no_dst() {
        // Source directory relative to CARGO_MANIFEST_DIR
        let err = Directory::from_str("src").unwrap_err();
        expect!["Can't parse a destination directory from 'src'"].assert_eq(&format!("{err}"));
    }

    #[test]
    fn test_directory_parsing_src_non_existent() {
        // Source directory relative to CARGO_MANIFEST_DIR
        let err = Directory::from_str("i_dont_exist:dst").unwrap_err();
        expect!["Can't parse an existing source directory from 'i_dont_exist'"]
            .assert_eq(&format!("{err}"));
    }

    #[test]
    fn test_directory_parsing_empty() {
        // Source directory relative to CARGO_MANIFEST_DIR
        let err = Directory::from_str("").unwrap_err();
        expect!["Can't parse a destination directory from ''"].assert_eq(&format!("{err}"));
    }
}
