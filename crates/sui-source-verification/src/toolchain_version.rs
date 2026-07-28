// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use move_package_alt::SourcePackageLayout;
use serde::Deserialize;

/// Read the compiler version from a legacy `Move.lock`'s `[move].toolchain-version` table.
///
/// The current package system records the toolchain version in the package's publication, which is
/// read through [`move_package_alt::read_publication`]. This covers packages published under the
/// older system (roughly v1.23 to v1.62), where the `Move.lock` is the only place the version
/// appears and the publication carries no such metadata. Returns `None` if there is no such entry.
pub(crate) fn legacy_move_lock_version(source_path: &Path) -> Option<String> {
    let contents =
        std::fs::read_to_string(source_path.join(SourcePackageLayout::Lock.path())).ok()?;

    #[derive(Deserialize)]
    struct Toolchain {
        #[serde(rename = "compiler-version")]
        compiler_version: String,
    }

    #[derive(Deserialize)]
    struct MoveSection {
        #[serde(rename = "toolchain-version")]
        toolchain_version: Option<Toolchain>,
    }

    #[derive(Deserialize)]
    struct Lock {
        #[serde(rename = "move")]
        move_: MoveSection,
    }

    let lock: Lock = toml::from_str(&contents).ok()?;
    lock.move_.toolchain_version.map(|t| t.compiler_version)
}
