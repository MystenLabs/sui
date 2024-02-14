// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use move_package::source_package::layout::SourcePackageLayout;
use std::{
    fmt::Display,
    fs::create_dir_all,
    io::Write,
    path::{Path, PathBuf},
};

// TODO get a stable path to this stdlib
// pub const MOVE_STDLIB_PACKAGE_NAME: &str = "MoveStdlib";
// pub const MOVE_STDLIB_PACKAGE_PATH: &str = "{ \
//     git = \"https://github.com/move-language/move.git\", \
//     subdir = \"language/move-stdlib\", rev = \"main\" \
// }";
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
    pub fn execute_with_defaults(self, path: Option<PathBuf>) -> anyhow::Result<()> {
        self.execute(
            path,
            std::iter::empty::<(&str, &str)>(),
            std::iter::empty::<(&str, &str)>(),
            "",
        )
    }

    pub fn execute(
        self,
        path: Option<PathBuf>,
        deps: impl IntoIterator<Item = (impl Display, impl Display)>,
        addrs: impl IntoIterator<Item = (impl Display, impl Display)>,
        custom: &str, // anything else that needs to end up being in Move.toml (or empty string)
    ) -> anyhow::Result<()> {
        // TODO warn on build config flags
        let Self { name } = self;
        let p: PathBuf;
        let path: &Path = match path {
            Some(path) => {
                p = path;
                &p
            }
            None => Path::new(&name),
        };
        create_dir_all(path.join(SourcePackageLayout::Sources.path()))?;
        let mut w = std::fs::File::create(path.join(SourcePackageLayout::Manifest.path()))?;
        writeln!(
            w,
            r#"[package]
name = "{name}"

# edition = "2024.alpha" # To use the Move 2024 edition, currently in alpha
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
