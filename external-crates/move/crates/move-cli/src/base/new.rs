// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{self, ensure, Context};
use clap::*;
use move_core_types::identifier::Identifier;
use move_package::source_package::layout::SourcePackageLayout;
use std::io::{BufRead, BufReader};
use std::{fmt::Display, fs::create_dir_all, io::Write, path::Path};

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
        self.execute(
            path,
            std::iter::empty::<(&str, &str)>(),
            std::iter::empty::<(&str, &str)>(),
            "",
        )
    }

    pub fn execute(
        self,
        path: Option<&Path>,
        deps: impl IntoIterator<Item = (impl Display, impl Display)>,
        addrs: impl IntoIterator<Item = (impl Display, impl Display)>,
        custom: &str, // anything else that needs to end up being in Move.toml (or empty string)
    ) -> anyhow::Result<()> {
        // TODO warn on build config flags

        ensure!(
            Identifier::is_valid(&self.name),
            "Invalid package name. Package name must start with a lowercase letter \
                     and consist only of lowercase letters, numbers, and underscores."
        );

        let path = path.unwrap_or_else(|| Path::new(&self.name));
        create_dir_all(path.join(SourcePackageLayout::Sources.path()))?;

        self.write_move_toml(path, deps, addrs, custom)?;
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
    fn write_move_toml(
        &self,
        path: &Path,
        deps: impl IntoIterator<Item = (impl Display, impl Display)>,
        addrs: impl IntoIterator<Item = (impl Display, impl Display)>,
        custom: &str, // anything else that needs to end up being in Move.toml (or empty string)
    ) -> anyhow::Result<()> {
        let Self { name } = self;

        let mut w = std::fs::File::create(path.join(SourcePackageLayout::Manifest.path()))?;
        writeln!(
            w,
            r#"[package]
name = "{name}"
edition = "2024.beta" # edition = "legacy" to use legacy (pre-2024) Move
# license = ""           # e.g., "MIT", "GPL", "Apache 2.0"
# authors = ["..."]      # e.g., ["Joe Smith (joesmith@noemail.com)", "John Snow (johnsnow@noemail.com)"]

[dependencies]"#
        )?;
        for (dep_name, dep_val) in deps {
            writeln!(w, "{dep_name} = {dep_val}")?;
        }

        writeln!(
            w,
            r#"
# For remote import, use the `{{ git = "...", subdir = "...", rev = "..." }}`.
# Revision can be a branch, a tag, and a commit hash.
# MyRemotePackage = {{ git = "https://some.remote/host.git", subdir = "remote/path", rev = "main" }}

# For local dependencies use `local = path`. Path is relative to the package root
# Local = {{ local = "../path/to" }}

# To resolve a version conflict and force a specific version for dependency
# override use `override = true`
# Override = {{ local = "../conflicting/version", override = true }}

[addresses]"#
        )?;

        // write named addresses
        for (addr_name, addr_val) in addrs {
            writeln!(w, "{addr_name} = \"{addr_val}\"")?;
        }

        writeln!(
            w,
            r#"
# Named addresses will be accessible in Move as `@name`. They're also exported:
# for example, `std = "0x1"` is exported by the Standard Library.
# alice = "0xA11CE"

[dev-dependencies]
# The dev-dependencies section allows overriding dependencies for `--test` and
# `--dev` modes. You can introduce test-only dependencies here.
# Local = {{ local = "../path/to/dev-build" }}

[dev-addresses]
# The dev-addresses section allows overwriting named addresses for the `--test`
# and `--dev` modes.
# alice = "0xB0B"
"#
        )?;

        // custom addition in the end
        if !custom.is_empty() {
            writeln!(w, "{}", custom)?;
        }

        Ok(())
    }
}
