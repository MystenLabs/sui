// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{self, Context};
use clap::*;
use indoc::formatdoc;
use move_package_alt::package::layout::SourcePackageLayout;
use move_package_alt::schema::PackageName;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
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

    pub fn execute(&self, path: Option<&Path>) -> anyhow::Result<()> {
        std::fs::write(
            self.source_file_path(&path)?,
            formatdoc!(
                r#"// For Move coding conventions, see
                // https://move-book.com/guides/code-quality-checklist

                /// Module: {name}
                module {name}::{name};


                public fun hello_world() {{

                }}"#,
                name = self.name_var()?,
            ),
        )?;
        self.write_move_toml(&path)?;
        self.write_gitignore(&path)?;
        Ok(())
    }

    /// add the following to `{path}/.gitignore` if it doesn't already have them:
    /// ```gitignore
    ///     build/*
    ///     .trace
    ///     .coverage*
    ///     Pub.*.toml
    /// ```
    fn write_gitignore(&self, path: &Option<&Path>) -> anyhow::Result<()> {
        let mut entries = vec!["build/*", ".trace", ".coverage*", "Pub.*.toml"];

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.gitignore_path(path)?)
            .context("Unexpected error creating .gitignore")?;

        for line in BufReader::new(&file).lines().map_while(Result::ok) {
            entries.retain(|e| *e != line);
        }

        for entry in entries {
            writeln!(file, "{entry}")?;
        }

        Ok(())
    }

    /// create default `Move.toml`
    fn write_move_toml(&self, path: &Option<&Path>) -> anyhow::Result<()> {
        std::fs::write(
            self.manifest_path(path)?,
            formatdoc!(
                r#"
                # Full documentation for Move.toml can be found at: docs.sui.io

                [package]
                name = "{name}"
                edition = "2024"         # use "2024" for Move 2024 edition
                # license = ""           # e.g., "MIT", "GPL", "Apache 2.0"
                # authors = ["..."]      # e.g., ["Joe Smith (joesmith@noemail.com)", "John Snow (johnsnow@noemail.com)"]
                # flavor = "sui"

                # add the environment names and their chain ids here
                # by default, testnet and mainnet are implicitly available
                # example for devnet: devnet = "abcdef1234"
                # [environments]
                # chain_name = "{{chain_id}}"

                # Add your dependencies here or leave empty (for adding automatically sui and std deps)
                # [dependencies]

                # Depedency on local package in the directory `../bar`, which can be referred to in the Move code as "bar::module::function"
                # bar = {{ local = "../bar" }}

                # Git dependency
                # foo = {{ git = "https://example.com/foo.git", rev = "releases/v1", subdir = "foo" }}

                # Setting `override = true` forces your dependencies to use this version of the package.
                # This is required if you need to link against a different version from one of your dependencies, or if
                # two of your dependencies depend on different versions of the same package
                # foo = {{ git = "https://example.com/foo.git", rev = "releases/v1", override = true }}

                # Use to replace dependencies for specific environments
                # [dep-replacements.mainnet]
                # foo = {{ git = "https://example.com/foo.git", original-id = "0x12g0cc1a418ff3bebce0ff9ec3961e6cc794af9bc3a4114fb138d00a4c9274bb", published-at = "0x12ga0cc1a418ff3bebce0ff9ec3961e6cc794af9bc3a4114fb138d00a4c9274bb", use-environment = "mainnet_beta" }}
                "#,
                name = self.name_var()?
            ),
        )?;

        Ok(())
    }

    pub fn gitignore_path(&self, path: &Option<&Path>) -> anyhow::Result<PathBuf> {
        Ok(self.root_dir(path)?.join(".gitignore"))
    }

    pub fn source_file_path(&self, path: &Option<&Path>) -> anyhow::Result<PathBuf> {
        let dir = self
            .root_dir(path)?
            .join(SourcePackageLayout::Sources.path());

        create_dir_all(&dir)?;
        Ok(dir.join(format!("{}.move", self.name_var()?)))
    }

    pub fn manifest_path(&self, path: &Option<&Path>) -> anyhow::Result<PathBuf> {
        Ok(self.root_dir(path)?.join("Move.toml"))
    }

    pub fn root_dir(&self, path: &Option<&Path>) -> anyhow::Result<PathBuf> {
        let result = path.unwrap_or_else(|| Path::new(&self.name)).to_path_buf();
        create_dir_all(&result)?;
        Ok(result)
    }

    pub fn name_var(&self) -> anyhow::Result<PackageName> {
        PackageName::new(self.name.to_lowercase())
    }
}
