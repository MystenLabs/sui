// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use rand::rngs::OsRng;
use tokio::sync::RwLock;

use crate::store::ForkingStore;
use simulacrum::Simulacrum;
use sui_pg_db::Db;
use sui_types::supported_protocol_versions::Chain;

#[derive(Clone)]
pub(crate) struct Context {
    pub pg_context: sui_indexer_alt_jsonrpc::context::Context,
    pub simulacrum: Arc<RwLock<Simulacrum<OsRng, ForkingStore>>>,
    pub db_writer: Db,
    pub at_checkpoint: u64,
    pub chain: Chain,
    pub protocol_version: u64,
}
