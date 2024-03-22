// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{anyhow, ensure, Context, Result};
use diesel::{
    r2d2::{ConnectionManager, Pool},
    PgConnection, RunQueryDsl,
};
use sui_indexer::{models::packages::StoredPackage, schema::packages};
use sui_types::{base_types::SuiAddress, move_package::MovePackage};
use tracing::info;

type PgPool = Pool<ConnectionManager<PgConnection>>;

pub(crate) async fn dump(db_url: String, output_dir: PathBuf) -> Result<()> {
    ensure_output_directory(&output_dir)?;

    let conn = ConnectionManager::<PgConnection>::new(db_url);
    let pool = Pool::builder()
        .max_size(1)
        .connection_timeout(Duration::from_secs(30))
        .build(conn)
        .context("Failed to create connection pool.")?;

    info!("Querying Indexer...");
    let pkgs = query_packages(&pool)?;
    let total = pkgs.len();

    let mut progress = 0;
    for (i, pkg) in pkgs.into_iter().enumerate() {
        let pct = (100 * i) / total;
        if pct % 5 == 0 && pct > progress {
            info!("Dumping packages ({total}): {pct: >3}%");
            progress = pct;
        }

        let id = SuiAddress::from_bytes(&pkg.package_id).context("Parsing package ID")?;
        dump_package(&output_dir, id, &pkg.move_package)
            .with_context(|| format!("Dumping package: {id}"))?;
    }

    info!("Dumping packages ({total}): 100%, Done.");
    Ok(())
}

/// Ensure the output directory exists, either because it already exists as an empty, writable
/// directory, or by creating a new directory.
fn ensure_output_directory(path: impl Into<PathBuf>) -> Result<()> {
    let path: PathBuf = path.into();
    if path.exists() {
        ensure!(
            path.is_dir(),
            "Output path is not a directory: {}",
            path.display()
        );
        ensure!(
            path.read_dir().is_ok_and(|mut d| d.next().is_none()),
            "Output directory is not empty: {}",
            path.display(),
        );

        let metadata = fs::metadata(&path).context("Getting metadata for output path")?;

        ensure!(
            !metadata.permissions().readonly(),
            "Output directory is not writable: {}",
            path.display()
        )
    } else {
        fs::create_dir_all(&path).context("Making output directory")?;
    }

    Ok(())
}

fn query_packages(pool: &PgPool) -> Result<Vec<StoredPackage>> {
    let mut conn = pool
        .get()
        .map_err(|e| anyhow!("Failed to get connection: {e}"))?;
    Ok(packages::dsl::packages.load::<StoredPackage>(&mut conn)?)
}

fn dump_package(output_dir: &Path, id: SuiAddress, pkg: &[u8]) -> Result<()> {
    let package = bcs::from_bytes::<MovePackage>(pkg).context("Deserializing")?;
    let origins: BTreeMap<_, _> = package
        .type_origin_table()
        .iter()
        .map(|o| {
            (
                format!("{}::{}", o.module_name, o.struct_name),
                o.package.to_string(),
            )
        })
        .collect();

    let package_dir = output_dir.join(format!("{}.{}", id, package.version().value()));
    fs::create_dir(&package_dir).context("Making output directory")?;

    let linkage_json =
        serde_json::to_string_pretty(package.linkage_table()).context("Serializing linkage")?;
    let origins_json =
        serde_json::to_string_pretty(&origins).context("Serializing type origins")?;

    fs::write(package_dir.join("package.bcs"), pkg).context("Writing package BCS")?;
    fs::write(package_dir.join("linkage.json"), linkage_json).context("Writing linkage")?;
    fs::write(package_dir.join("origins.json"), origins_json).context("Writing type origins")?;

    for (module_name, module_bytes) in package.serialized_module_map() {
        let module_path = package_dir.join(format!("{module_name}.mv"));
        fs::write(module_path, module_bytes)
            .with_context(|| format!("Writing module: {module_name}"))?
    }

    Ok(())
}
