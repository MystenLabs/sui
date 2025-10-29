// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    package::EnvironmentName,
    schema::{MoveHeader, OriginalID, ParsedLockfile, PublishAddresses, PublishedID, RenderToml},
};
use anyhow::{Result, anyhow};
use colored::Colorize;
use std::{collections::BTreeMap, path::Path};
use toml::Value as TV;
use tracing::warn;

use super::{legacy::LegacyEnvironment, parse_address_literal};

/// Parse the legacy lockfile in `path` (i.e. version 3 or less) and return the extracted
/// information.
///
/// If the file doesn't exist or isn't a legacy lockfile, returns `Ok(None)`
/// If the file exists but can't be parsed as a legacy lockfile, returns an error
/// If the file exists and is a weird mishmash of a legacy and modern lockfile, we replace it with
///    a modern lockfile (after emitting a loud warning); in this case we also return `Ok(None)`
pub fn load_legacy_lockfile(
    lockfile_path: &Path,
) -> anyhow::Result<Option<BTreeMap<EnvironmentName, LegacyEnvironment>>> {
    let Ok(file_contents) = std::fs::read_to_string(lockfile_path) else {
        return Ok(None);
    };

    // check the [move.version] field
    let toml_val = toml::from_str::<TV>(&file_contents)?;
    let lockfile = toml_val
        .as_table()
        .ok_or(anyhow!("Could not parse lockfile: expected a toml table"))?;

    let header = lockfile
        .get("move")
        .and_then(|v| v.as_table())
        .ok_or(anyhow!(
            "Could not parse lockfile: expected a [move] section"
        ))?;

    let version = header
        .get("version")
        .and_then(|v| v.as_integer())
        .unwrap_or(0);

    // Ignore modern lock files
    if version > 3 {
        return Ok(None);
    }

    // The old package system didn't fail if the lockfile version field was too high, instead it
    // just mushed the lockfile fields in with the new lockfile fields. We detect this case and
    // complain loudly (and preserve the modern pins)
    if let Some(pinned) = lockfile.get("pinned") {
        warn!(
            "{}: Detected a modern lockfile at {lockfile_path:?} that was modified by an older CLI; some information may be lost. Be sure that all contributors are using the latest CLI.",
            "WARNING".bold().yellow()
        );

        let lockfile = ParsedLockfile {
            header: MoveHeader::default(),
            pinned: pinned.clone().try_into()?,
        };

        // TODO: this really should be handled by the output path, but that requires effort
        std::fs::write(lockfile_path, lockfile.render_as_toml())?;

        return Ok(None);
    };

    // Extract legacy addresses and write them into the pub file
    let publications: BTreeMap<EnvironmentName, LegacyEnvironment> =
        parse_legacy_lockfile_addresses(lockfile)?;

    Ok(Some(publications))
}

fn parse_legacy_lockfile_addresses(
    lockfile: &toml::map::Map<String, toml::Value>,
) -> Result<BTreeMap<EnvironmentName, LegacyEnvironment>> {
    let mut published = BTreeMap::new();

    // Extract the environments as a table.
    let Some(envs) = lockfile.get("env").and_then(|v| v.as_table()) else {
        return Ok(published);
    };

    for (name, data) in envs {
        let env_name = name.to_string();
        let env_table = data.as_table().unwrap();

        let chain_id = env_table
            .get("chain-id")
            .map(|v| v.as_str().unwrap_or_default().to_string());
        let original_id = env_table
            .get("original-published-id")
            .map(|v| parse_address_literal(v.as_str().unwrap_or_default()).unwrap());
        let latest_id = env_table
            .get("latest-published-id")
            .map(|v| parse_address_literal(v.as_str().unwrap_or_default()).unwrap());

        let published_version = env_table
            .get("published-version")
            .map(|v| v.as_str().unwrap_or_default().to_string())
            .and_then(|v| v.parse::<u64>().ok());

        if let (Some(chain_id), Some(original_id), Some(latest_id), Some(published_version)) =
            (chain_id, original_id, latest_id, published_version)
        {
            published.insert(
                env_name,
                LegacyEnvironment {
                    chain_id,
                    addresses: PublishAddresses {
                        original_id: OriginalID(original_id),
                        published_at: PublishedID(latest_id),
                    },
                    version: published_version,
                },
            );
        }
    }

    Ok(published)
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::load_legacy_lockfile;
    use crate::{
        flavor::Vanilla,
        schema::{ParsedPublishedFile, RenderToml},
    };
    use move_command_line_common::testing::insta::assert_snapshot;
    use test_log::test;

    /// Loading a legacy lockfile with no addresses returns an empty collection
    #[test(tokio::test)]
    async fn convert_no_envs() {
        let tempdir = tempfile::tempdir().unwrap();
        let lockfile = tempdir.path().join("Move.lock");
        std::fs::write(
            &lockfile,
            indoc!(
                r#"
                # @generated by Move, please check-in and do not edit manually.
                [move]
                version = 1
                manifest_digest = "E7FF1D7441FA0105B981EE018AEF168A18B22984DEABBF2F111AA6FBB3C3CB81"
                deps_digest = "3C4103934B1E040BB6B23F1D610B4EF9F2F1166A50A104EADCF77467C004C600"
            "#
            ),
        )
        .unwrap();

        let pubs = load_legacy_lockfile(&lockfile).unwrap();
        assert!(pubs.unwrap().is_empty());
    }

    /// Converting a legacy manifest containing envs converts them into a pubfile
    #[tokio::test]
    async fn convert_with_envs() {
        let tempdir = tempfile::tempdir().unwrap();
        let lockfile = tempdir.path().join("Move.lock");
        std::fs::write(
            &lockfile,
            indoc!(
                r###"
                # @generated by Move, please check-in and do not edit manually.
                [move]
                version = 1

                [env.mainnet]
                chain-id = "35834a8a"
                original-published-id = "0x2"
                latest-published-id = "0x3"
                published-version = "3"

                [env.testnet]
                chain-id = "4c78adac"
                original-published-id = "0x5"
                latest-published-id = "0x6"
                published-version = "2"
            "###
            ),
        )
        .unwrap();

        let pubs: ParsedPublishedFile<Vanilla> =
            load_legacy_lockfile(&lockfile).unwrap().unwrap().into();

        assert_snapshot!(pubs.render_as_toml(), @r###"
        # Generated by Move
        # This file contains metadata about published versions of this package in different environments
        # This file SHOULD be committed to source control

        [published.mainnet]
        chain-id = "35834a8a"
        published-at = "0x0000000000000000000000000000000000000000000000000000000000000003"
        original-id = "0x0000000000000000000000000000000000000000000000000000000000000002"
        version = 3

        [published.testnet]
        chain-id = "4c78adac"
        published-at = "0x0000000000000000000000000000000000000000000000000000000000000006"
        original-id = "0x0000000000000000000000000000000000000000000000000000000000000005"
        version = 2
        "###);
    }

    /// Converting a modern manifest has no effect
    #[tokio::test]
    async fn convert_modern() {
        let tempdir = tempfile::tempdir().unwrap();
        let lockfile = tempdir.path().join("Move.lock");
        std::fs::write(
            &lockfile,
            indoc!(
                r###"
                [move]
                version = 4
            "###
            ),
        )
        .unwrap();

        let pubs = load_legacy_lockfile(&lockfile).unwrap();
        assert!(pubs.is_none());
    }

    /// Converting a malformed manifest produces an error and has no effect on the file
    #[tokio::test]
    async fn convert_malformed() {
        let tempdir = tempfile::tempdir().unwrap();
        let lockfile = tempdir.path().join("Move.lock");
        std::fs::write(&lockfile, indoc!(r###"[moooove]"###)).unwrap();

        let pubs = load_legacy_lockfile(&lockfile);

        assert_snapshot!(pubs.unwrap_err().to_string(), @"Could not parse lockfile: expected a [move] section");
    }

    /// Loading a very old lockfile that doesn't have a version field succeeds
    /// DVX-1766
    #[tokio::test]
    async fn convert_unversioned_lockfile() {
        let tempdir = tempfile::tempdir().unwrap();
        let lockfile = tempdir.path().join("Move.lock");
        std::fs::write(
            &lockfile,
            indoc!(
                r###"
                [move]
                manifest_digest = "46963749C976A052F2770EA6625F4DF4366F72291DC73139750BC416CF77A247"
                deps_digest = "3C4103934B1E040BB6B23F1D610B4EF9F2F1166A50A104EADCF77467C004C600"
                dependencies = [
                  { name = "P" },
                  { name = "Sui" },
                ]

                [[move.package]]
                name = "P"
                source = { git = "foo.git", rev = "main", subdir = "" }
                dependencies = []

                [move.toolchain-version]
                compiler-version = "1.30.1"
                edition = "2024.beta"
                flavor = "sui"
                "###
            ),
        )
        .unwrap();

        let pubs = load_legacy_lockfile(&lockfile).unwrap();
        assert!(pubs.unwrap().is_empty());
    }

    /// Loading a mooshed lockfile replaces it with a modern one and returns `None`
    #[tokio::test]
    async fn convert_mooshed_lockfile() {
        let tempdir = tempfile::tempdir().unwrap();
        let lockfile = tempdir.path().join("Move.lock");
        std::fs::write(
            &lockfile,
            indoc!(
                r###"
                [move]
                manifest_digest = "46963749C976A052F2770EA6625F4DF4366F72291DC73139750BC416CF77A247"
                deps_digest = "3C4103934B1E040BB6B23F1D610B4EF9F2F1166A50A104EADCF77467C004C600"
                dependencies = [
                  { name = "P" },
                  { name = "Sui" },
                ]

                [[move.package]]
                name = "P"
                source = { git = "foo.git", rev = "main", subdir = "" }
                dependencies = []

                [move.toolchain-version]
                compiler-version = "1.30.1"
                edition = "2024.beta"
                flavor = "sui"

                [pinned.testnet.Sui]
                source = { git = "...", subdir = "...", rev = "da39a3ee5e6b4b0d3255bfef95601890afd80709" }
                use_environment = "testnet"
                manifest_digest = "ED5DEFBBF556EE89312E639A53F21DE24320F9B13C2087D3BFE2989D5B2B5DAF"
                deps = {}

                [pinned.testnet.foo]
                source = { git = "...", subdir = "...", rev = "da39a3ee5e6b4b0d3255bfef95601890afd80709" }
                use_environment = "testnet"
                manifest_digest = "ED5DEFBBF556EE89312E639A53F21DE24320F9B13C2087D3BFE2989D5B2B5DAF"
                deps = { sui = "Sui" }
                "###
            ),
        )
        .unwrap();

        let pubs = load_legacy_lockfile(&lockfile).unwrap();
        assert!(pubs.is_none());

        let new_lockfile = std::fs::read_to_string(&lockfile).unwrap();
        assert_snapshot!(new_lockfile, @r###"
        # Generated by move; do not edit
        # This file should be checked in.

        [move]
        version = 4

        [pinned.testnet.Sui]
        source = { git = "...", subdir = "...", rev = "da39a3ee5e6b4b0d3255bfef95601890afd80709" }
        use_environment = "testnet"
        manifest_digest = "ED5DEFBBF556EE89312E639A53F21DE24320F9B13C2087D3BFE2989D5B2B5DAF"
        deps = {}

        [pinned.testnet.foo]
        source = { git = "...", subdir = "...", rev = "da39a3ee5e6b4b0d3255bfef95601890afd80709" }
        use_environment = "testnet"
        manifest_digest = "ED5DEFBBF556EE89312E639A53F21DE24320F9B13C2087D3BFE2989D5B2B5DAF"
        deps = { sui = "Sui" }
        "###);
    }
}
