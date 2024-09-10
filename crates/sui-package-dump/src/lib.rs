// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, ensure, Context, Result};
use client::Client;
use fastcrypto::encoding::{Base64, Encoding};
use query::{limits, packages, SuiAddress, UInt53};
use sui_types::object::Object;
use tracing::info;

mod client;
mod query;

/// Ensure all packages created before `before_checkpoint` are written to the `output_dir`ectory,
/// from the GraphQL service at `rpc_url`.
///
/// `output_dir` can be a path to a non-existent directory, an existing empty directory, or an
/// existing directory written to in the past. If the path is non-existent, the invocation creates
/// it. If the path exists but is empty, the invocation writes to the directory. If the directory
/// has been written to in the past, the invocation picks back up where the previous invocation
/// left off.
pub async fn dump(
    rpc_url: String,
    output_dir: PathBuf,
    before_checkpoint: Option<u64>,
) -> Result<()> {
    ensure_output_directory(&output_dir)?;

    let client = Client::new(rpc_url)?;
    let after_checkpoint = read_last_checkpoint(&output_dir)?;
    let limit = max_page_size(&client).await?;
    let (last_checkpoint, packages) =
        fetch_packages(&client, limit, after_checkpoint, before_checkpoint).await?;

    for package in &packages {
        let SuiAddress(address) = &package.address;
        dump_package(&output_dir, package)
            .with_context(|| format!("Failed to dump package {address}"))?;
    }

    if let Some(last_checkpoint) = last_checkpoint {
        write_last_checkpoint(&output_dir, last_checkpoint)?;
    }

    Ok(())
}

/// Ensure the output directory exists, either because it already exists as a writable directory, or
/// by creating a new directory.
fn ensure_output_directory(path: impl Into<PathBuf>) -> Result<()> {
    let path: PathBuf = path.into();
    if !path.exists() {
        fs::create_dir_all(&path).context("Making output directory")?;
        return Ok(());
    }

    ensure!(
        path.is_dir(),
        "Output path is not a directory: {}",
        path.display()
    );

    let metadata = fs::metadata(&path).context("Getting metadata for output path")?;

    ensure!(
        !metadata.permissions().readonly(),
        "Output directory is not writable: {}",
        path.display()
    );

    Ok(())
}

/// Load the last checkpoint that was loaded by a previous run of the tool, if there is a previous
/// run.
fn read_last_checkpoint(output: &Path) -> Result<Option<u64>> {
    let path = output.join("last-checkpoint");
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).context("Failed to read last checkpoint")?;
    let checkpoint: u64 =
        serde_json::from_str(&content).context("Failed to parse last checkpoint")?;

    info!("Resuming download after checkpoint {checkpoint}");

    Ok(Some(checkpoint))
}

/// Write the max checkpoint that we have seen a package from back to the output directory.
fn write_last_checkpoint(output: &Path, checkpoint: u64) -> Result<()> {
    let path = output.join("last-checkpoint");
    let content =
        serde_json::to_string(&checkpoint).context("Failed to serialize last checkpoint")?;

    fs::write(path, content).context("Failed to write last checkpoint")?;
    Ok(())
}

/// Read the max page size supported by the GraphQL service.
async fn max_page_size(client: &Client) -> Result<i32> {
    Ok(client
        .query(limits::build())
        .await
        .context("Failed to fetch max page size")?
        .service_config
        .max_page_size)
}

/// Read all the packages between `after_checkpoint` and `before_checkpoint`, in batches of
/// `page_size` from the `client` connected to a GraphQL service.
///
/// If `after_checkpoint` is not provided, packages are read from genesis. If `before_checkpoint`
/// is not provided, packages are read until the latest checkpoint.
///
/// Returns the latest checkpoint that was read from in this fetch, and a list of all the packages
/// that were read.
async fn fetch_packages(
    client: &Client,
    page_size: i32,
    after_checkpoint: Option<u64>,
    before_checkpoint: Option<u64>,
) -> Result<(Option<u64>, Vec<packages::MovePackage>)> {
    let packages::Query {
        checkpoint: checkpoint_viewed_at,
        packages:
            packages::MovePackageConnection {
                mut page_info,
                mut nodes,
            },
    } = client
        .query(packages::build(
            page_size,
            None,
            after_checkpoint.map(UInt53),
            before_checkpoint.map(UInt53),
        ))
        .await
        .with_context(|| "Failed to fetch page 1 of packages.")?;

    for i in 2.. {
        if !page_info.has_next_page {
            break;
        }

        let packages = client
            .query(packages::build(
                page_size,
                page_info.end_cursor,
                after_checkpoint.map(UInt53),
                before_checkpoint.map(UInt53),
            ))
            .await
            .with_context(|| format!("Failed to fetch page {i} of packages."))?
            .packages;

        nodes.extend(packages.nodes);
        page_info = packages.page_info;

        info!(
            "Fetched page {i} ({} package{} so far).",
            nodes.len(),
            if nodes.len() == 1 { "" } else { "s" },
        );
    }

    use packages::Checkpoint as C;
    let last_checkpoint = match (checkpoint_viewed_at, before_checkpoint) {
        (
            Some(C {
                sequence_number: UInt53(v),
            }),
            Some(b),
        ) if b > 0 => Some(v.min(b - 1)),
        (
            Some(C {
                sequence_number: UInt53(c),
            }),
            _,
        )
        | (_, Some(c)) => Some(c),
        _ => None,
    };

    Ok((last_checkpoint, nodes))
}

/// Write out `pkg` to the `output_dir`ectory, using the package's address and name as the directory
/// name. The following files are written for each directory:
///
/// - `object.bcs` -- the BCS serialized form of the `Object` type containing the package.
///
/// - `linkage.json` -- a JSON serialization of the package's linkage table, mapping dependency
///   original IDs to the version of the dependency being depended on and the ID of the object
///   on chain that contains that version.
///
/// - `origins.json` -- a JSON serialization of the type origin table, mapping type names contained
///   in this package to the version of the package that first introduced that type.
///
/// - `*.mv` -- a BCS serialization of each compiled module in the package.
fn dump_package(output_dir: &Path, pkg: &packages::MovePackage) -> Result<()> {
    let Some(query::Base64(bcs)) = &pkg.bcs else {
        bail!("Missing BCS");
    };

    let bytes = Base64::decode(bcs).context("Failed to decode BCS")?;

    let object = bcs::from_bytes::<Object>(&bytes).context("Failed to deserialize")?;
    let id = object.id();
    let Some(package) = object.data.try_as_package() else {
        bail!("Not a package");
    };

    let origins: BTreeMap<_, _> = package
        .type_origin_table()
        .iter()
        .map(|o| {
            (
                format!("{}::{}", o.module_name, o.datatype_name),
                o.package.to_string(),
            )
        })
        .collect();

    let package_dir = output_dir.join(format!("{}.{}", id, package.version().value()));
    fs::create_dir(&package_dir).context("Failed to make output directory")?;

    let linkage_json = serde_json::to_string_pretty(package.linkage_table())
        .context("Failed to serialize linkage")?;
    let origins_json =
        serde_json::to_string_pretty(&origins).context("Failed to serialize type origins")?;

    fs::write(package_dir.join("object.bcs"), bytes).context("Failed to write object BCS")?;
    fs::write(package_dir.join("linkage.json"), linkage_json).context("Failed to write linkage")?;
    fs::write(package_dir.join("origins.json"), origins_json)
        .context("Failed to write type origins")?;

    for (module_name, module_bytes) in package.serialized_module_map() {
        let module_path = package_dir.join(format!("{module_name}.mv"));
        fs::write(module_path, module_bytes)
            .with_context(|| format!("Failed to write module: {module_name}"))?
    }

    Ok(())
}
