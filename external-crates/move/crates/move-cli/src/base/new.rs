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
    ///     traces/*
    ///     .trace
    ///     .coverage*
    ///     Pub.*.toml
    /// ```
    fn write_gitignore(&self, path: &Option<&Path>) -> anyhow::Result<()> {
        let mut entries = vec!["build/*", "traces/*", ".trace", ".coverage*", "Pub.*.toml"];

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
        let name = self.name_var()?;
        std::fs::write(
            self.manifest_path(path)?,
            formatdoc!(
                r#"[package]
            name = "{name}"
            edition = "2024"         # edition = "legacy" to use legacy (pre-2024) Move
            # license = ""           # e.g., "MIT", "GPL", "Apache 2.0"
            # authors = ["..."]      # e.g., ["Joe Smith (joesmith@noemail.com)", "John Snow (johnsnow@noemail.com)"]

            [dependencies]

            # For remote import, use the `{{ git = "...", subdir = "...", rev = "..." }}`.
            # Revision can be a branch, a tag, and a commit hash.
            # myremotepackage = {{ git = "https://some.remote/host.git", subdir = "remote/path", rev = "main" }}

            # For local dependencies use `local = path`. Path is relative to the package root
            # local = {{ local = "../path/to" }}

            # To resolve a version conflict and force a specific version for dependency
            # override use `override = true`
            # override = {{ local = "../conflicting/version", override = true }}

            [addresses]
            {name} = "0x0"
            # Named addresses will be accessible in Move as `@name`. They're also exported:
            # for example, `std = "0x1"` is exported by the Standard Library.
            # alice = "0xA11CE"

            [dev-dependencies]
            # The dev-dependencies section allows overriding dependencies for `--test` and
            # `--dev` modes. You can introduce test-only dependencies here.
            # local = {{ local = "../path/to/dev-build" }}

            [dev-addresses]
            # The dev-addresses section allows overwriting named addresses for the `--test`
            # and `--dev` modes.
            # alice = "0xB0B"
            "#
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
