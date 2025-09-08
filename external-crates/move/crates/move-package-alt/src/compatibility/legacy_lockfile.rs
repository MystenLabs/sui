// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    flavor::MoveFlavor,
    package::{EnvironmentName, paths::PackagePath},
    schema::{
        OriginalID, ParsedLocalPubs, ParsedLockfile, Publication, PublishAddresses, PublishedID,
        RenderToml,
    },
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
    let published: BTreeMap<_, _> = parse_legacy_lockfile_addresses(lockfile)?
        .into_iter()
        .map(|(name, env)| {
            (
                name,
                Publication::<F> {
                    chain_id: env.chain_id,
                    addresses: env.addresses,
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

        let pubfile = ParsedLocalPubs { published };
        std::fs::write(path.publications_path(), pubfile.render_as_toml())?;
    }

    // Write an (empty) modern lockfile
    //   TODO: maybe we should try to extract and preserve dependency information?
    let lockfile = ParsedLockfile::default();
    std::fs::write(path.lockfile_path(), lockfile.render_as_toml())?;

    Ok(())
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
            .map(|v| v.as_str().unwrap_or_default().to_string());

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
    use test_log::test;

    /// Converting a legacy manifest with no addresses just clobbers it
    #[test]
    fn convert_no_envs() {
        todo!()
    }

    /// Converting a legacy manifest containing envs converts them into a pubfile
    #[test]
    fn convert_with_envs() {
        todo!()
    }

    /// Converting a modern manifest has no effect
    #[test]
    fn convert_modern() {
        todo!()
    }

    /// Converting a malformed manifest produces an error and has no effect on the file
    #[test]
    fn convert_malformed() {
        todo!()
    }
}
