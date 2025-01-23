// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::dataloader::DataLoader;
use prometheus::Registry;
use sui_indexer_alt_framework::{
    ingestion::{ClientArgs, IngestionConfig},
    Indexer, IndexerArgs,
};
use sui_indexer_alt_schema::MIGRATIONS;
use sui_pg_db::{temp::TempDb, DbArgs};
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

use crate::data::reader::Reader;
use crate::metrics::RpcMetrics;

pub(crate) struct IndexerReaderTestEnv {
    pub(crate) indexer: Indexer,
    pub(crate) reader: Reader,
    pub(crate) _temp_db: TempDb,
}

impl IndexerReaderTestEnv {
    pub(crate) async fn new() -> Self {
        let temp_db = TempDb::new().unwrap();
        let db_args = DbArgs::new_for_testing(temp_db.database().url().clone());
        let registry = Registry::new();
        let indexer = Indexer::new(
            db_args.clone(),
            IndexerArgs::default(),
            ClientArgs {
                remote_store_url: None,
                local_ingestion_path: Some(tempdir().unwrap().into_path()),
            },
            IngestionConfig::default(),
            &MIGRATIONS,
            &registry,
            CancellationToken::new(),
        )
        .await
        .unwrap();
        let rpc_metrics = RpcMetrics::new(&registry);
        let reader = Reader::new(db_args, rpc_metrics, &registry).await.unwrap();
        Self {
            indexer,
            reader,
            _temp_db: temp_db,
        }
    }

    pub(crate) fn loader(&self) -> DataLoader<Reader> {
        self.reader.as_data_loader()
    }
}
