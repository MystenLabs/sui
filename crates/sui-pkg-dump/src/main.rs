use std::{collections::BTreeMap, fs, path::{PathBuf, Path}, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use clap::*;
use diesel::{
    r2d2::{ConnectionManager, Pool},
    PgConnection, RunQueryDsl,
};
use sui_indexer::{models_v2::packages::StoredPackage, schema_v2::packages};
use sui_types::{base_types::SuiAddress, move_package::MovePackage};

#[derive(Parser)]
#[clap(
    name = "sui-pkg-dump",
    about = "\
    Download all packages to the local filesystem from an indexer database.  Each package gets its \
    own sub-directory, named for its ID on-chain, containing two metadata files (linkage.json and \
    origins.json) as well as a file for every module it contains.  Each module file is named for \
    its module name, with a .mv suffix, and contains Move bytecode (suitable for passing into a \
    debugger).",
    rename_all = "kebab-case",
    author,
    version
)]
struct Args {
    /// Connection information for the Indexer's Postgres DB.
    #[clap(long, short)]
    db_url: String,

    /// Path to a non-existent directory that can be created and filled with package information.
    #[clap(long, short)]
    output_dir: PathBuf,
}

type PgPool = Pool<ConnectionManager<PgConnection>>;

#[tokio::main]
async fn main() -> Result<()> {
    let Args { db_url, output_dir } = Args::parse();

    if !is_valid_output(&output_dir) {
        bail!("Output directory already exists: {}", output_dir.display())
    } else {
        fs::create_dir_all(&output_dir).context("Making output directory")?;
    }

    let conn = ConnectionManager::<PgConnection>::new(db_url);
    let pool = Pool::builder()
        .max_size(1)
        .connection_timeout(Duration::from_secs(30))
        .build(conn)
        .context("Failed to create connection pool.")?;

    println!("Dumping packages...");
    for pkg in query_packages(&pool)? {
        let id = SuiAddress::from_bytes(&pkg.package_id).context("Parsing package ID")?;
        dump_package(&output_dir, id, &pkg.move_package)
            .with_context(|| format!("Dumping package: {id}"))?;
    }

    Ok(())
}

/// A non-existent or empty directory is a valid output directory
fn is_valid_output(path: impl Into<PathBuf>) -> bool {
    let path: PathBuf = path.into();
    !path.exists() || path.is_dir() && path.read_dir().is_ok_and(|mut d| d.next().is_none())
}

fn query_packages(pool: &PgPool) -> Result<Vec<StoredPackage>> {
    let mut conn = pool
        .get()
        .map_err(|e| anyhow!("Failed to get connection: {e}"))?;
    Ok(packages::dsl::packages.load::<StoredPackage>(&mut conn)?)
}

fn dump_package(output_dir: &Path, id: SuiAddress, pkg: &[u8]) -> Result<()> {
    let package_dir = output_dir.join(id.to_string());

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

    fs::create_dir(&package_dir).context("Making output directory")?;

    let linkage_json = serde_json::to_string_pretty(package.linkage_table())
        .context("Serializing linkage")?;
    let origins_json = serde_json::to_string_pretty(&origins)
        .context("Serializing type origins")?;

    fs::write(package_dir.join("linkage.json"), linkage_json).context("Writing linkage")?;
    fs::write(package_dir.join("origins.json"), origins_json).context("Writing type origins")?;

    for (module_name, module_bytes) in package.serialized_module_map() {
        let module_path = package_dir.join(format!("{module_name}.mv"));
        fs::write(module_path, module_bytes)
            .with_context(|| format!("Writing module: {module_name}"))?
    }

    Ok(())
}
