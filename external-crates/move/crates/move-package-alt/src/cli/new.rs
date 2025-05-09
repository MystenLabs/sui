// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs::create_dir_all,
    io::Write,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use crate::errors::PackageResult;
use anyhow::{Context, ensure};
use clap::{Command, Parser, Subcommand};
use move_core_types::identifier::Identifier;
use move_package::source_package::layout::SourcePackageLayout;

/// Build the package
#[derive(Debug, Clone, Parser)]
pub struct New {
    name: String,
    /// Path to the project
    path: Option<PathBuf>,
}

impl New {
    pub fn execute(&self) -> PackageResult<()> {
        if !Identifier::is_valid(&self.name) {
            return Err(crate::errors::PackageError::Generic(
                "Invalid package name. Package name must start with a letter or underscore \
                     and consist only of letters, numbers, and underscores."
                    .to_string(),
            ));
        }

        let path = match self.path {
            Some(ref path) => path,
            None => {
                let current_dir = std::env::current_dir()?;
                &current_dir.join(&self.name)
            }
        };

        // create module source file
        let mut w = std::fs::File::create(
            path.join(SourcePackageLayout::Sources.path())
                .join(format!("{}.move", self.name)),
        )?;
        writeln!(
            w,
            r#"/*
/// Module: {name}
module {name}::{name};
*/

// For Move coding conventions, see
// https://docs.sui.io/concepts/sui-move-concepts/conventions
"#,
            name = self.name
        )?;

        create_dir_all(path.join(SourcePackageLayout::Sources.path()))?;

        self.write_move_toml(path);
        self.write_gitignore(path)?;
        Ok(())
    }

    /// add `build/*` to `{path}/.gitignore` if it doesn't already have it
    fn write_gitignore(&self, path: &Path) -> anyhow::Result<()> {
        let gitignore_entry = "build/*";

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.join(".gitignore"))
            .context("Unexpected error creating .gitignore")?;

        for line in BufReader::new(&file).lines().map_while(Result::ok) {
            if line == gitignore_entry {
                return Ok(());
            }
        }

        writeln!(file, "{gitignore_entry}")?;
        Ok(())
    }

    /// create default `Move.toml`
    fn write_move_toml(&self, path: &Path) -> anyhow::Result<()> {
        let Self { name, path: _ } = self;

        let mut w = std::fs::File::create(path.join(SourcePackageLayout::Manifest.path()))?;
        let toml_content = r#"[package]
name = "{name}"
edition = "2024" # edition = "legacy" to use legacy (pre-2024) Move
# license = ""           # e.g., "MIT", "GPL", "Apache 2.0"
# authors = ["..."]      # e.g., ["Joe Smith (joesmith@noemail.com)", "John Snow (johnsnow@noemail.com)"]
# implicit-deps = true
# flavor = sui

[environments] # add the environment names and their chain ids here
mainnet = "35834a8a"
# testnet = "4c78adac"

[dependencies]
# Add your dependencies here or leave empty and set implicit-deps true above

# Local dep
# bar_local = { path = "<path>" }

# Git dependency
# foo = { git = "https://example.com/foo.git", rev = "releases/v1", rename-from = "Foo"}

# To resolve a version conflict and force a specific version for dependency
# override use `override = true`
# foo = { git = "https://example.com/foo.git", rev = "releases/v1", rename-from = "Foo", override = true}

# External dep via mvr
# qwer = { r.mvr = "@pkg/qwer" }

[dep-overrides]
# used to override dependencies for specific environments
# mainnet.foo = { git = "https://example.com/foo.git", original-id = "0x6ba0cc1a418ff3bebce0ff9ec3961e6cc794af9bc3a4114fb138d00a4c9274bb", published-at = "0x6ba0cc1a418ff3bebce0ff9ec3961e6cc794af9bc3a4114fb138d00a4c9274bb", use-environment = "mainnet_alpha" }

[dep-overrides.mainnet.foo]
git = "https://example.com/foo.git"
original-id = "0x12g0cc1a418ff3bebce0ff9ec3961e6cc794af9bc3a4114fb138d00a4c9274bb"
published-at = "0x12ga0cc1a418ff3bebce0ff9ec3961e6cc794af9bc3a4114fb138d00a4c9274bb"
use-environment = "mainnet_beta"
""#;

        let toml_content = toml_content.replace("{name}", name);
        let toml_path = path.join("Move.toml");
        std::fs::write(&toml_path, toml_content)?;

        Ok(())
    }
}
