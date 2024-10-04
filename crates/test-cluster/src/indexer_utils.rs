// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer::tempdb::TempDb;
use tempfile::TempDir;

pub(crate) struct IndexerHandle {
    pub cancellation_tokens: Vec<tokio_util::sync::DropGuard>,
    pub data_ingestion_dir: Option<TempDir>,
    pub database: TempDb,
}
