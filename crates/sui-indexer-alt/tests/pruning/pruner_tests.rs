// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::query_dsl::select_dsl::SelectDsl;
use diesel_async::RunQueryDsl;
use rand::rngs::StdRng;
use rand::SeedableRng;
use simulacrum::Simulacrum;
use std::{ops::Range, path::PathBuf, time::Duration};
use sui_indexer_alt::{
    config::{IndexerConfig, Merge},
    start_indexer,
};
use sui_indexer_alt_framework::{
    ingestion::ClientArgs, models::cp_sequence_numbers::tx_interval, IndexerArgs,
};
use sui_indexer_alt_schema::schema::{kv_checkpoints, kv_epoch_starts};
use sui_pg_db::{
    temp::{get_available_port, TempDb},
    Connection, Db, DbArgs,
};
use tempfile::TempDir;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Prepares a test indexer configuration, deferring to the default config for top-level fields, but
/// explicitly setting all fields of the `cp_sequence_numbers` pipeline layer. This can then be
/// passed to the `indexer_config` arg of `start_indexer`.
fn load_indexer_config(path_str: &str) -> IndexerConfig {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push(format!("tests/pruning/configs/{}", path_str));
    let config_str = std::fs::read_to_string(&path)
        .expect(&format!("Failed to read test config file at {:?}", path));
    toml::from_str(&config_str).expect("Failed to parse test config TOML")
}

/// The TempDir and TempDb need to be kept alive for the duration of the test, otherwise parts of
/// the test env will hang indefinitely.
async fn setup_temp_resources() -> (TempDb, TempDir) {
    let temp_db = TempDb::new().unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    (temp_db, temp_dir)
}

async fn setup_test_env(
    db_url: String,
    data_ingestion_path: PathBuf,
    indexer_config: IndexerConfig,
) -> (
    Simulacrum<StdRng>,
    Db,
    JoinHandle<anyhow::Result<()>>,
    CancellationToken,
) {
    // Set up simulacrum
    let rng = StdRng::from_seed([12; 32]);
    let mut sim = Simulacrum::new_with_rng(rng);
    sim.set_data_ingestion_path(data_ingestion_path.clone());

    // Set up direct db pool for test assertions
    let db = Db::for_write(DbArgs {
        database_url: db_url.parse().unwrap(),
        db_connection_pool_size: 1,
        connection_timeout_ms: 60_000,
    })
    .await
    .unwrap();

    // Set up indexer
    let db_args = DbArgs {
        database_url: db_url.parse().unwrap(),
        db_connection_pool_size: 10,
        connection_timeout_ms: 60_000,
    };

    let prom_address = format!("127.0.0.1:{}", get_available_port())
        .parse()
        .unwrap();
    let indexer_args = IndexerArgs {
        metrics_address: prom_address,
        ..Default::default()
    };

    let client_args = ClientArgs {
        remote_store_url: None,
        local_ingestion_path: Some(data_ingestion_path),
    };

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    // Spawn the indexer in a separate task
    let indexer_handle = tokio::spawn(async move {
        start_indexer(
            db_args,
            indexer_args,
            client_args,
            indexer_config,
            true,
            Some(cancel_clone),
        )
        .await
    });

    (sim, db, indexer_handle, cancel)
}

/// Even though the indexer consists of several independent pipelines, the `cp_sequence_numbers`
/// table governs checkpoint -> tx and epoch lookups and provides such information for prunable
/// tables. This waits for the lookup table to be updated with the expected changes.
async fn wait_for_tx_interval(
    conn: &mut Connection<'_>,
    duration: Duration,
    cp_range: Range<u64>,
) -> anyhow::Result<()> {
    tokio::select! {
        _ = tokio::time::sleep(duration) => {
            anyhow::bail!("Timeout occurred while waiting for tx interval of checkpoints [{}, {})", cp_range.start, cp_range.end);
        }
        result = async {
            loop {
                match tx_interval(conn, cp_range.clone()).await {
                    Ok(_) => break Ok(()),
                    Err(_) => {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        } => result
    }
}

async fn cleanup_test_env(
    cancel: CancellationToken,
    indexer_handle: JoinHandle<anyhow::Result<()>>,
) {
    cancel.cancel();
    let _ = indexer_handle.await.expect("Indexer task panicked");
}

/// Test that the `cp_sequence_numbers` is correctly committed to.
#[tokio::test]
pub async fn test_cp_sequence_numbers() -> () {
    let indexer_config = load_indexer_config("base_config.toml");
    let cp_sequence_numbers_config = load_indexer_config("cp_sequence_numbers.toml");
    let merged_config = indexer_config.merge(cp_sequence_numbers_config);
    let (temp_db, temp_dir) = setup_temp_resources().await;
    let db_url = temp_db.database().url().as_str().to_owned();
    let data_ingestion_path = temp_dir.path().to_path_buf();

    let (mut sim, db, indexer_handle, cancel) =
        setup_test_env(db_url, data_ingestion_path, merged_config).await;

    sim.create_checkpoint();
    sim.create_checkpoint();

    let mut conn = db
        .connect()
        .await
        .expect("Failed to retrieve DB connection");

    if let Err(e) = wait_for_tx_interval(&mut conn, Duration::from_secs(5), 0..2).await {
        cleanup_test_env(cancel, indexer_handle).await;
        panic!("{:?}", e);
    }

    cleanup_test_env(cancel, indexer_handle).await;
}

#[tokio::test]
pub async fn test_kv_epoch_starts_cross_epoch() -> () {
    let merged_config = load_indexer_config("base_config.toml")
        .merge(load_indexer_config("cp_sequence_numbers.toml"))
        .merge(load_indexer_config("kv_epoch_starts_diff_epoch.toml"));
    let (temp_db, temp_dir) = setup_temp_resources().await;
    let db_url = temp_db.database().url().as_str().to_owned();
    let data_ingestion_path = temp_dir.path().to_path_buf();

    let (mut sim, db, indexer_handle, cancel) =
        setup_test_env(db_url, data_ingestion_path, merged_config).await;

    sim.advance_epoch(true);
    sim.advance_epoch(true);
    sim.advance_epoch(true);

    let mut conn = db
        .connect()
        .await
        .expect("Failed to retrieve DB connection");

    if let Err(e) = wait_for_tx_interval(&mut conn, Duration::from_secs(5), 0..3).await {
        cleanup_test_env(cancel, indexer_handle).await;
        panic!("{:?}", e);
    }

    let timeout_duration = Duration::from_secs(5);
    tokio::select! {
        _ = tokio::time::sleep(timeout_duration) => {
            cancel.cancel();
            return;
        }
        _ = async {
            loop {
                match kv_epoch_starts::table
                    .select(kv_epoch_starts::epoch)
                    .load::<i64>(&mut conn)
                    .await
                {
                    Ok(epochs) if epochs == vec![3] => break,
                    Ok(_) | Err(_) => {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        } => {}
    }

    cleanup_test_env(cancel, indexer_handle).await;
}

/// The checkpoint-based pruner watermark continuously updates the `pruner_hi`, but we don't want to
/// prune epoch-related data until the `[from, to)` checkpoints are across epochs.
#[tokio::test]
pub async fn test_kv_epoch_starts_same_epoch() -> () {
    let merged_config = load_indexer_config("base_config.toml")
        .merge(load_indexer_config("cp_sequence_numbers.toml"))
        .merge(load_indexer_config("kv_epoch_starts_diff_epoch.toml"));
    let (temp_db, temp_dir) = setup_temp_resources().await;
    let db_url = temp_db.database().url().as_str().to_owned();
    let data_ingestion_path = temp_dir.path().to_path_buf();

    let (mut sim, db, indexer_handle, cancel) =
        setup_test_env(db_url, data_ingestion_path, merged_config).await;

    sim.advance_epoch(true);
    sim.create_checkpoint();
    sim.create_checkpoint();
    sim.create_checkpoint();

    let mut conn = db
        .connect()
        .await
        .expect("Failed to retrieve DB connection");

    if let Err(e) = wait_for_tx_interval(&mut conn, Duration::from_secs(5), 0..4).await {
        cleanup_test_env(cancel, indexer_handle).await;
        panic!("{:?}", e);
    }

    let timeout_duration = Duration::from_secs(5);
    tokio::select! {
        _ = tokio::time::sleep(timeout_duration) => {
            cancel.cancel();
            return;
        }
        _ = async {
            loop {
                match kv_epoch_starts::table
                    .select(kv_epoch_starts::epoch)
                    .load::<i64>(&mut conn)
                    .await
                {
                    Ok(epochs) if epochs == vec![1] => break,
                    Ok(_) | Err(_) => {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        } => {}
    }

    cleanup_test_env(cancel, indexer_handle).await;
}

/// Not all tables require a mapping to the `cp_sequence_numbers` table. For example,
/// `kv_checkpoints` table can be pruned directly with checkpoint-based watermarks from the pruner
/// watermark task. In this test, the `cp_sequence_numbers` table is not enabled. The indexer should
/// still be able to prune `kv_checkpoints`.
#[tokio::test]
pub async fn test_kv_checkpoints_no_mapping() -> () {
    let merged_config =
        load_indexer_config("base_config.toml").merge(load_indexer_config("kv_checkpoints.toml"));
    let (temp_db, temp_dir) = setup_temp_resources().await;
    let db_url = temp_db.database().url().as_str().to_owned();
    let data_ingestion_path = temp_dir.path().to_path_buf();

    let (mut sim, db, indexer_handle, cancel) =
        setup_test_env(db_url, data_ingestion_path, merged_config).await;

    sim.create_checkpoint();
    sim.create_checkpoint();
    sim.create_checkpoint();

    let mut conn = db
        .connect()
        .await
        .expect("Failed to retrieve DB connection");

    let timeout_duration = Duration::from_secs(5);
    tokio::select! {
        _ = tokio::time::sleep(timeout_duration) => {
            cancel.cancel();
            return;
        }
        _ = async {
            loop {
                match kv_checkpoints::table
                    .select(kv_checkpoints::sequence_number)
                    .load::<i64>(&mut conn)
                    .await
                {
                    Ok(checkpoints) if checkpoints == vec![3] => break,
                    Ok(_) | Err(_) => {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        } => {}
    }

    cancel.cancel();

    // Wait for the indexer to shut down
    let _ = indexer_handle.await.expect("Indexer task panicked");
}
