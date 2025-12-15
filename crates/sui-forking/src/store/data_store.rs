use sui_data_store::{
    Node,
    stores::{DataStore, FileSystemStore, LruMemoryStore, ReadThroughStore},
};

/// Create a new layered data store with memory, disk, and GraphQL layers.
///
/// VERSION is the crate version from `bin_version` crate.
pub fn new_rpc_data_store(
    node: Node,
    version: &str,
) -> Result<
    ReadThroughStore<LruMemoryStore, ReadThroughStore<FileSystemStore, DataStore>>,
    anyhow::Error,
> {
    let graphql = DataStore::new(node.clone(), version)?;
    let disk = FileSystemStore::new(node.clone())?;
    let disk_with_remote = ReadThroughStore::new(disk, graphql);
    let memory = LruMemoryStore::new(node);
    let store = ReadThroughStore::new(memory, disk_with_remote);

    Ok(store)
}
