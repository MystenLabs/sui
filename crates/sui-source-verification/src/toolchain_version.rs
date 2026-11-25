// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use move_package::SourcePackageLayout;
use serde::Deserialize;

use crate::error::Error;

pub(crate) fn parse(version: &str) -> Result<(u32, u32, u32), Error> {
    let number = |part: &str| -> Option<u32> {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };

    let parse = || -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = version.split('.').collect();
        let &[major, minor, patch] = parts.as_slice() else {
            return None;
        };
        Some((number(major)?, number(minor)?, number(patch)?))
    };

    parse().ok_or_else(|| Error::InvalidToolchainVersion {
        version: version.to_string(),
    })
}

/// Read the compiler version from a legacy `Move.lock`'s `[move].toolchain-version` table.
///
/// The current package system records the toolchain version in the package's publication, which is
/// read through [`move_package::read_publication`]. This covers packages published under the
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

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::error::Error;

    #[test]
    fn parses_release_versions() {
        assert_eq!(parse("1.80.1").unwrap(), (1, 80, 1));
        assert_eq!(parse("1.9.0").unwrap(), (1, 9, 0));
        assert_eq!(parse("0.0.0").unwrap(), (0, 0, 0));
    }

    #[test]
    fn rejects_versions_that_are_not_release_versions() {
        let invalid = [
            "",
            "1",
            "1.80",
            "1.80.1.0",
            "1..1",
            ".80.1",
            "1.80.",
            ".",
            "..",
            "nightly",
            "1.80.x",
            "v1.80.1",
            "+1.80.1",
            "-1.80.1",
            "1.80.1-rc1",
            " 1.80.1",
            "1.80.1 ",
            "1.80.1\n",
            "1.80.1\0",
            "١.٨٠.١",
            "4294967296.0.0",
            "1.80.1/a",
            "1.80.1/../a",
            "1.80.1\\a",
            "1.80.1/%2e%2e/a",
            "1.80.1?a",
            "1.80.1#a",
            "../1.80.1",
            "/tmp/1.80.1",
            "C:\\1.80.1",
        ];

        for version in invalid {
            let result = parse(version);
            assert!(
                matches!(result, Err(Error::InvalidToolchainVersion { .. })),
                "{version:?}: {result:?}"
            );
        }
    }

    #[test]
    fn error_message_escapes_the_version() {
        let message = parse("1.80.1\u{1b}[2J").unwrap_err().to_string();
        assert!(message.contains(r#""1.80.1\u{1b}[2J""#), "{message}");
    }
}
