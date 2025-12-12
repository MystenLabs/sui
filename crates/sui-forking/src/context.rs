// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::rngs::OsRng;
use std::sync::{Arc, RwLock};

use async_graphql::dataloader::DataLoader;
use prometheus::Registry;
use tokio_util::sync::CancellationToken;
use url::Url;

use sui_indexer_alt_reader::{
    bigtable_reader::{BigtableArgs, BigtableReader},
    kv_loader::KvLoader,
    package_resolver::{DbPackageStore, PackageCache},
    pg_reader::PgReader,
    pg_reader::db::DbArgs,
};
use sui_package_resolver::Resolver;

use simulacrum::Simulacrum;
use sui_data_store::stores::{DataStore, FileSystemStore, LruMemoryStore, ReadThroughStore};
use sui_indexer_alt_jsonrpc::{config::RpcConfig, metrics::RpcMetrics};
use sui_types::supported_protocol_versions::Chain;

use crate::store::ForkingStore;

#[derive(Clone)]
pub(crate) struct Context {
    pub pg_context: sui_indexer_alt_jsonrpc::context::Context,
    pub simulacrum: Arc<RwLock<Simulacrum<OsRng, ForkingStore>>>,
    pub at_checkpoint: u64,
    pub chain: Chain,
    pub protocol_version: u64,
}
