// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Interactive database shell for the Sui validator database.
//!
//! Two operating modes:
//!
//! **Direct mode** (`--db-path`): opens RocksDB files directly. Requires the
//! node to be stopped. Read-only by default; pass `--allow-writes` to enable rm.
//!
//! **Proxy mode** (`--admin-url`): delegates all operations to the running
//! sui-node admin API. Allows write operations because the node owns the DB lock.

pub mod backend;
pub mod completion;
pub mod direct;
pub mod proxy;
pub mod shell;
pub mod vfs;

use anyhow::{Context, bail};
use clap::Parser;
use consensus_core::storage::rocksdb_store::RocksDBStore;
use std::path::PathBuf;
use std::sync::Arc;
use sui_core::{
    authority::{
        authority_store_pruner::PrunerWatermarks,
        authority_store_tables::AuthorityPerpetualTables,
    },
    checkpoints::CheckpointStore,
    epoch::committee_store::CommitteeStore,
};
use sui_types::committee::Committee;

use self::{backend::Backend, direct::DirectBackend, proxy::ProxyBackend};

#[derive(Parser, Debug)]
#[command(
    name = "db-shell",
    about = "Interactive shell for navigating the validator database",
    long_about = r#"Interactive shell for navigating the Sui validator database.

Two modes of operation:

  Direct mode  (--db-path): opens the database files directly.
               The node must NOT be running.

  Proxy mode   (--admin-url): proxies all operations through the running
               node's admin API, allowing safe concurrent access.

If both flags are given, proxy mode takes precedence.

Curl-compatible API (proxy mode):
  curl 'http://127.0.0.1:1337/db-shell/ls?path=/checkpoints/seq&limit=10'
  curl 'http://127.0.0.1:1337/db-shell/read?path=/checkpoints/seq/1/summary&format=json'
  curl 'http://127.0.0.1:1337/db-shell/read?path=/checkpoints/seq/1/summary&format=debug'
  curl 'http://127.0.0.1:1337/db-shell/read?path=/checkpoints/seq/1/summary&format=bcs'
"#
)]
pub struct DbShellArgs {
    /// Path to the validator database directory (direct mode, node must be stopped).
    #[arg(long)]
    pub db_path: Option<PathBuf>,

    /// Admin API URL of the running sui-node (proxy mode).
    /// Example: http://127.0.0.1:1337
    #[arg(long)]
    pub admin_url: Option<String>,

    /// Initial working directory (default: /).
    #[arg(long, default_value = "/")]
    pub start_path: String,

    /// Path to the consensus database directory (direct mode only).
    /// Enables the /consensus namespace. Typically at <config_dir>/consensus_db/<key>.
    #[arg(long)]
    pub consensus_db_path: Option<PathBuf>,
}

pub fn run(args: DbShellArgs) -> anyhow::Result<()> {
    let backend: Arc<dyn Backend> = match (&args.admin_url, &args.db_path) {
        (Some(url), _) => {
            eprintln!("Connecting to sui-node admin API at {url}");
            Arc::new(ProxyBackend::new(url)?)
        }
        (None, Some(db_path)) => {
            eprintln!("Opening database at {}", db_path.display());
            // CheckpointStore::new already returns Arc<CheckpointStore>.
            let checkpoint_store = CheckpointStore::new(
                &db_path.join("checkpoints"),
                Arc::new(PrunerWatermarks::default()),
            );

            // CommitteeStore requires a genesis committee to initialize, but we're
            // opening an existing database so it will already be populated.
            // We pass a dummy genesis committee; it is only used when the DB is empty.
            let dummy_genesis = Committee::new_simple_test_committee_of_size(0).0;
            let committee_store = Arc::new(CommitteeStore::new(
                db_path.join("epochs"),
                &dummy_genesis,
                None,
            ));

            let authority_tables = Arc::new(AuthorityPerpetualTables::open(
                &db_path.join("store"),
                None,
                None,
            ));

            let consensus_store = if let Some(path) = &args.consensus_db_path {
                let path_str = path
                    .to_str()
                    .with_context(|| format!("invalid consensus DB path: {}", path.display()))?;
                eprintln!("Opening consensus database at {path_str}");
                Some(Arc::new(RocksDBStore::new(path_str)))
            } else {
                None
            };

            Arc::new(DirectBackend {
                checkpoint_store,
                committee_store,
                authority_tables,
                consensus_store,
            })
        }
        (None, None) => {
            bail!("specify either --db-path <path> (direct mode) or --admin-url <url> (proxy mode)")
        }
    };

    let initial_cwd = vfs::parse_path(&args.start_path)
        .with_context(|| format!("invalid start path: '{}'", args.start_path))?;

    shell::run_shell(backend, initial_cwd)
}
