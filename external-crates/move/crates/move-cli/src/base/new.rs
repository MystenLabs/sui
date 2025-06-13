// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{self, Context, ensure};
use clap::*;
use move_core_types::identifier::Identifier;
use move_package_alt::package::layout::SourcePackageLayout;
use std::io::{BufRead, BufReader};
use std::{fs::create_dir_all, io::Write, path::Path};

pub const MOVE_STDLIB_ADDR_NAME: &str = "std";
pub const MOVE_STDLIB_ADDR_VALUE: &str = "0x1";

/// Create a new Move package with name `name` at `path`. If `path` is not provided the package
/// will be created in the directory `name`.
#[derive(Parser)]
#[clap(name = "new")]
pub struct New {
    /// The name of the package to be created.
    pub name: String,
}

impl New {
    pub fn execute_with_defaults(self, path: Option<&Path>) -> anyhow::Result<()> {
        self.execute(path)
    }

    pub fn execute(self, path: Option<&Path>) -> anyhow::Result<()> {
        // TODO warn on build config flags

        ensure!(
            Identifier::is_valid(&self.name),
            "Invalid package name. Package name must start with a letter or underscore \
                     and consist only of letters, numbers, and underscores."
        );

        let path = path.unwrap_or_else(|| Path::new(&self.name));
        create_dir_all(path.join(SourcePackageLayout::Sources.path()))?;
        let mut w = std::fs::File::create(
            path.join(SourcePackageLayout::Sources.path())
                .join(format!("{}.move", self.name)),
        )?;

        writeln!(
            w,
            r#"// For Move coding conventions, see
// https://docs.sui.io/concepts/sui-move-concepts/conventions

/// Module: {name}
module {name}::{name};


public fun hello_world() {{

}}"#,
            name = self.name
        )?;
        self.write_move_toml(path)?;
        self.write_gitignore(path)?;
        Ok(())
    }

    /// add `build/*` to `{path}/.gitignore` if it doesn't already have it
    fn write_gitignore(&self, path: &Path) -> anyhow::Result<()> {
        let gitignore_entry = "build/*\n.trace\n.coverage*";

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
        let Self { name } = self;

        let _ = std::fs::File::create(path.join(SourcePackageLayout::Manifest.path()))?;
        let toml_content = r#"# Full documentation for Move.toml can be found at: docs.sui.io

[package]
name = "{name}"
edition = "2024"         # use "2024" for Move 2024 edition
# license = ""           # e.g., "MIT", "GPL", "Apache 2.0"
# authors = ["..."]      # e.g., ["Joe Smith (joesmith@noemail.com)", "John Snow (johnsnow@noemail.com)"]
# flavor = sui

[environments]           # add the environment names and their chain ids here
mainnet = "{MAINNET_CHAIN_ID}"
testnet = "{TESTNET_CHAIN_ID}"

[dependencies]
# Add your dependencies here or leave empty.

# Depedency on local package in the directory `../bar`, which can be referred to in the Move code as "bar::module::function"
# bar = { local = "../bar" }

# Git dependency
# foo = { git = "https://example.com/foo.git", rev = "releases/v1", subdir = "foo" }

# Setting `override = true` forces your dependencies to use this version of the package.
# This is required if you need to link against a different version from one of your dependencies, or if
# two of your dependencies depend on different versions of the same package
# foo = { git = "https://example.com/foo.git", rev = "releases/v1", override = true}

[dep-replacements.mainnet]
# Use to replace dependencies for specific environments

foo = { git = "https://example.com/foo.git", original-id = "0x12g0cc1a418ff3bebce0ff9ec3961e6cc794af9bc3a4114fb138d00a4c9274bb", published-at = "0x12ga0cc1a418ff3bebce0ff9ec3961e6cc794af9bc3a4114fb138d00a4c9274bb", use-environment = "mainnet_beta" }"#;

        let toml_content = toml_content.replace("{name}", &name.to_string());
        let toml_path = path.join("Move.toml");
        std::fs::write(&toml_path, toml_content)?;

        Ok(())
    }
}
