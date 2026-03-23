// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Result;
use sui_data_store::{
    CheckpointStore, CheckpointStoreWriter, EpochStore, EpochStoreWriter, ObjectStore,
    ObjectStoreWriter, SetupStore, StoreSummary, TransactionStore, TransactionStoreWriter,
    node::Node,
    stores::{
        CompositeStore, DataStore, FileSystemStore, InMemoryStore, LruMemoryStore,
        ReadThroughStore, WriteThroughStore,
    },
};

fn temp_store_path() -> Result<PathBuf> {
    Ok(tempfile::tempdir()?.into_path())
}

fn assert_read_write_store<T>()
where
    T: TransactionStore
        + TransactionStoreWriter
        + EpochStore
        + EpochStoreWriter
        + ObjectStore
        + ObjectStoreWriter
        + CheckpointStore
        + CheckpointStoreWriter
        + SetupStore
        + StoreSummary,
{
}

fn assert_read_only_store<T>()
where
    T: TransactionStore + EpochStore + ObjectStore + CheckpointStore + SetupStore + StoreSummary,
{
}

#[test]
fn backends_instantiate() -> Result<()> {
    let _graphql = DataStore::new(Node::Mainnet, "test-version")?;
    let _filesystem = FileSystemStore::new_with_path(Node::Mainnet, temp_store_path()?)?;
    let _memory = InMemoryStore::new(Node::Mainnet);
    let _lru = LruMemoryStore::new(Node::Mainnet);
    Ok(())
}

#[test]
fn backend_trait_surfaces_compile() {
    assert_read_only_store::<DataStore>();
    assert_read_write_store::<FileSystemStore>();
    assert_read_write_store::<InMemoryStore>();
    assert_read_write_store::<LruMemoryStore>();
}

#[test]
fn combinators_instantiate() -> Result<()> {
    let filesystem = FileSystemStore::new_with_path(Node::Mainnet, temp_store_path()?)?;
    let graphql = DataStore::new(Node::Mainnet, "test-version")?;
    let read_through = ReadThroughStore::new(filesystem, graphql);

    let filesystem = FileSystemStore::new_with_path(Node::Mainnet, temp_store_path()?)?;
    let memory = InMemoryStore::new(Node::Mainnet);
    let write_through = WriteThroughStore::new(memory, filesystem);

    let composite = CompositeStore::new(
        InMemoryStore::new(Node::Mainnet),
        InMemoryStore::new(Node::Mainnet),
        InMemoryStore::new(Node::Mainnet),
        InMemoryStore::new(Node::Mainnet),
    );

    let _ = read_through.primary();
    let _ = write_through.secondary();
    let _ = composite.objects();
    Ok(())
}

#[test]
fn combinator_trait_surfaces_compile() {
    assert_read_only_store::<ReadThroughStore<FileSystemStore, DataStore>>();
    assert_read_write_store::<WriteThroughStore<InMemoryStore, FileSystemStore>>();
    assert_read_write_store::<CompositeStore<InMemoryStore, InMemoryStore, InMemoryStore, InMemoryStore>>();
}
