// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    flavor::MoveFlavor,
    package::{EnvironmentName, paths::PackagePath},
    schema::{OriginalID, ParsedPubs, Publication, PublishAddresses, PublishedID, RenderToml},
};
use anyhow::{Result, anyhow, bail};
use std::collections::BTreeMap;
use toml::Value as TV;

use super::{legacy::LegacyEnvironment, parse_address_literal};

/// Check whether the lockfile in `path` is a legacy format (i.e. version 3 or less); if so,
/// replace it with a modern lockfile and pubfile. If the file is present but cannot be read, (e.g.
/// the file is corrupted), return an error.
pub fn convert_legacy_lockfile<F: MoveFlavor>(path: &PackagePath) -> Result<()> {
    if !path.lockfile_path().exists() {
        return Ok(());
    }

    // check the [move.version] field
    let file_contents = std::fs::read_to_string(path.lockfile_path())?;
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
        .ok_or(anyhow!(
            "Could not parse lockfile: expected an integer `version` field"
        ))?;

    // Ignore modern lock files
    if version > 3 {
        return Ok(());
    }

    // Extract legacy addresses and write them into the pub file
    let published: BTreeMap<_, Publication<F>> = parse_legacy_lockfile_addresses(lockfile)?
        .into_iter()
        .map(|(name, env)| {
            (
                name,
                Publication {
                    chain_id: env.chain_id,
                    addresses: env.addresses,
                    version: env.version,
                    metadata: F::PublishedMetadata::default(),
                },
            )
        })
        .collect();

    if !published.is_empty() {
        if path.publications_path().exists() {
            bail!(
                "Could not extract addresses from the legacy lockfile into publications file because
                {:?} already exists",
                path.publications_path()
            )
        }

        let pubfile = ParsedPubs { published };
        std::fs::write(path.publications_path(), pubfile.render_as_toml())?;
    }

    // Delete the legacy lockfile to only trigger this codepath once.
    std::fs::remove_file(path.lockfile_path()).map_err(|e| anyhow!(e))
}

/// Loads the `[env]` table from a legacy lockfile.
pub(crate) fn try_load_legacy_lockfile_publications(
    path: &PackagePath,
) -> Result<BTreeMap<EnvironmentName, LegacyEnvironment>> {
    let file_contents = std::fs::read_to_string(path.lockfile_path())?;
    let toml_val = toml::from_str::<TV>(&file_contents)?;
    let lockfile = toml_val
        .as_table()
        .ok_or(anyhow!("Could not parse lockfile: expected a toml table"))?;

    parse_legacy_lockfile_addresses(lockfile)
}

fn parse_legacy_lockfile_addresses(
    lockfile: &toml::value::Map<String, toml::Value>,
) -> Result<BTreeMap<EnvironmentName, LegacyEnvironment>> {
    let mut publish_info = BTreeMap::new();

    // Extract the environments as a table.
    let Some(envs) = lockfile.get("env").and_then(|v| v.as_table()) else {
        return Ok(publish_info);
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
            publish_info.insert(
                env_name,
                LegacyEnvironment {
                    addresses: PublishAddresses {
                        original_id: OriginalID(original_id),
                        published_at: PublishedID(latest_id),
                    },
                    chain_id,
                    version: published_version,
                },
            );
        }
    }

    Ok(publish_info)
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use std::{fs::File, io::Write};

    use super::convert_legacy_lockfile;
    use crate::{
        compatibility::legacy_lockfile::try_load_legacy_lockfile_publications, flavor::Vanilla,
    };
    use move_command_line_common::testing::insta::assert_snapshot;
    use move_core_types::account_address::AccountAddress;
    use tempfile::{TempDir, tempdir};
    use test_log::test;
    use tokio::fs;

    use crate::package::paths::PackagePath;

    fn setup() -> (TempDir, PackagePath) {
        let dir = tempdir().unwrap();
        // create move.toml to make this a solid PackagePath.
        let _ = File::create(dir.path().join("Move.toml")).unwrap();
        let package_path = PackagePath::new(dir.path().to_path_buf()).unwrap();
        (dir, package_path)
    }

    /// Converting a legacy manifest with no addresses just clobbers it
    #[test]
    fn convert_no_envs() {
        let (_tmpdir, package_path) = setup();

        let mut file = File::create(package_path.lockfile_path()).unwrap();

        writeln!(
            file,
            r#"
        # @generated by Move, please check-in and do not edit manually.
[move]
version = 1
manifest_digest = "E7FF1D7441FA0105B981EE018AEF168A18B22984DEABBF2F111AA6FBB3C3CB81"
deps_digest = "3C4103934B1E040BB6B23F1D610B4EF9F2F1166A50A104EADCF77467C004C600"
"#
        )
        .unwrap();

        convert_legacy_lockfile::<Vanilla>(&package_path).unwrap();
        // lockfile was deleted.
        assert!(!package_path.lockfile_path().exists());
        // publications file was not created as lockfile didn't have any data.
        assert!(!package_path.publications_path().exists());
    }

    /// Converting a legacy manifest containing envs converts them into a pubfile
    #[tokio::test]
    async fn convert_with_envs() {
        let (_tmpdir, package_path) = setup();

        let mut file = File::create(package_path.lockfile_path()).unwrap();

        writeln!(
            file,
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
            )
        )
        .unwrap();

        convert_legacy_lockfile::<Vanilla>(&package_path).unwrap();

        // lockfile was deleted.
        assert!(!package_path.lockfile_path().exists());
        // publications file was created
        assert!(package_path.publications_path().exists());

        let contents = fs::read_to_string(package_path.publications_path())
            .await
            .unwrap();

        assert_snapshot!(contents, @r###"
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
        let (_tmpdir, package_path) = setup();

        let mut file = File::create(package_path.lockfile_path()).unwrap();

        writeln!(
            file,
            r#"
        [move]
        version = 4
        "#
        )
        .unwrap();

        convert_legacy_lockfile::<Vanilla>(&package_path).unwrap();

        // lockfile was not deleted.
        assert!(package_path.lockfile_path().exists());
        // publications file was not created
        assert!(!package_path.publications_path().exists());

        // verify no side-effects on lockfile

        assert_snapshot!( fs::read_to_string(package_path.lockfile_path()).await.unwrap(), @r###"
        [move]
        version = 4
        "###);
    }

    /// Converting a malformed manifest produces an error and has no effect on the file
    #[tokio::test]
    async fn convert_malformed() {
        let (_tmpdir, package_path) = setup();
        let mut file = File::create(package_path.lockfile_path()).unwrap();

        writeln!(file, r#"[mooove]"#).unwrap();

        let result = convert_legacy_lockfile::<Vanilla>(&package_path);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Could not parse lockfile: expected a [move] section"
        );

        // lockfile was not deleted.
        assert!(package_path.lockfile_path().exists());
        // publications file was not created
        assert!(!package_path.publications_path().exists());

        assert_snapshot!(fs::read_to_string(package_path.lockfile_path()).await.unwrap(), @r#"[mooove]"#);
    }

    #[test]
    // Test a simple load of the lockfile, for the "legacy" case.
    fn load_legacy_publications() {
        let (_tmpdir, package_path) = setup();
        let mut file = File::create(package_path.lockfile_path()).unwrap();
        writeln!(
            file,
            indoc!(
                r###"
                [env.mainnet]
                chain-id = "35834a8a"
                original-published-id = "0x2"
                latest-published-id = "0x3"
                published-version = "3"
                "###
            )
        )
        .unwrap();

        let environments = try_load_legacy_lockfile_publications(&package_path).unwrap();

        let mainnet = environments.get("mainnet").unwrap();
        assert_eq!(environments.len(), 1);
        assert_eq!(mainnet.chain_id, "35834a8a");
        assert_eq!(
            mainnet.addresses.original_id.0,
            AccountAddress::from_hex_literal("0x2").unwrap()
        );
        assert_eq!(
            mainnet.addresses.published_at.0,
            AccountAddress::from_hex_literal("0x3").unwrap()
        );
        assert_eq!(mainnet.version, 3);
    }
}
