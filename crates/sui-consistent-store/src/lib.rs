// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Type-safe wrapper around RocksDB for Sui's on-disk indexes,
//! plus the glue that drives indexing pipelines into it.
//!
//! Provides the encoding traits, the database wrapper, typed point
//! reads, atomic batched writes, typed iteration, and in-memory
//! snapshots. On top of that, exposes:
//!
//! - [`Restore`] — pipelines populate themselves from a stream of
//!   live objects (formal snapshot restore, perpetual-store
//!   restore).
//! - [`Store`] + [`Connection`] — implementations of the
//!   indexer-alt framework's `Store` / `SequentialStore` traits, so
//!   pipelines that implement the framework's
//!   `Processor` + `sequential::Handler` traits can be driven by
//!   the alt framework against a [`Db`].
//! - [`Synchronizer`] — coordinates writes from multiple pipelines
//!   into a single [`Db`], taking cross-pipeline snapshots at
//!   stride boundaries.
//!
//! [`rocksdb`] is re-exported so consumers can construct the
//! [`rocksdb::Options`] / [`rocksdb::WriteOptions`] values the
//! public API takes without adding a direct rocksdb dependency
//! themselves.
pub use rocksdb;

pub mod batch;
mod committer_watermark;
pub mod db;
pub mod encode;
mod encode_buf;
pub mod error;
pub mod framework;
pub mod iter;
pub mod map;
pub mod metrics;
pub mod options;
pub mod proto;
pub mod protobuf;
pub mod reader;
pub mod restore;
pub mod schema;
pub mod snapshot;
pub mod store;
pub mod synchronizer;

pub use crate::batch::Batch;
pub use crate::db::Db;
pub use crate::db::DbOptions;
pub use crate::db::DbRef;
pub use crate::db::RocksMetrics;
pub use crate::encode::Decode;
pub use crate::encode::Encode;
pub use crate::framework::ChainId;
pub use crate::framework::FrameworkSchema;
pub use crate::framework::PipelineTaskKey;
pub use crate::framework::RestoreState;
pub use crate::framework::Watermark;
pub use crate::framework::restore_state;
pub use crate::iter::Iter;
pub use crate::iter::RevIter;
pub use crate::map::DbMap;
pub use crate::options::CfOptionsResolver;
pub use crate::options::CfTuning;
pub use crate::options::Compression;
pub use crate::options::DbWideConfig;
pub use crate::options::RocksDbConfig;
pub use crate::options::WriteStallConfig;
pub use crate::protobuf::Protobuf;
pub use crate::reader::Reader;
pub use crate::restore::Restore;
pub use crate::schema::CfDescriptor;
pub use crate::schema::Schema;
pub use crate::schema::SchemaAtSnapshot;
pub use crate::snapshot::Snapshot;
pub use crate::store::Connection;
pub use crate::store::Store;
pub use crate::synchronizer::Synchronizer;
