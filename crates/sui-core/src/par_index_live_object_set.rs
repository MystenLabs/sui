// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store_tables::LiveObject;
use crate::authority::AuthorityStore;
use std::time::Instant;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;
use sui_types::storage::error::Error as StorageError;
use tracing::info;

/// Make `LiveObjectIndexer`s for parallel indexing of the live object set
pub trait ParMakeLiveObjectIndexer: Sync {
    type ObjectIndexer: LiveObjectIndexer;

    fn make_live_object_indexer(&self) -> Self::ObjectIndexer;
}

/// Represents an instance of a indexer that operates on a subset of the live object set
pub trait LiveObjectIndexer {
    /// Called on each object in the range of the live object set this indexer task is responsible
    /// for.
    fn index_object(&mut self, object: Object) -> Result<(), StorageError>;

    /// Called once the range of objects this indexer task is responsible for have been processed
    /// by calling `index_object`.
    fn finish(self) -> Result<(), StorageError>;
}

/// Utility for iterating over, and indexing, the live object set in parallel
///
/// This is done by dividing the addressable ObjectID space into smaller, disjoint sets and
/// operating on each set in parallel in a separate thread. User's will need to implement the
/// `ParMakeLiveObjectIndexer` trait which will be used to make N `LiveObjectIndexer`s which will
/// then process one of the disjoint parts of the live object set.
pub fn par_index_live_object_set<T: ParMakeLiveObjectIndexer>(
    authority_store: &AuthorityStore,
    make_indexer: &T,
) -> Result<(), StorageError> {
    info!("Indexing Live Object Set");
    let start_time = Instant::now();
    std::thread::scope(|s| -> Result<(), StorageError> {
        let mut threads = Vec::new();
        const BITS: u8 = 5;
        for index in 0u8..(1 << BITS) {
            threads.push(s.spawn(move || {
                let object_indexer = make_indexer.make_live_object_indexer();
                live_object_set_index_task(index, BITS, authority_store, object_indexer)
            }));
        }

        // join threads
        for thread in threads {
            thread.join().unwrap()?;
        }

        Ok(())
    })?;

    info!(
        "Indexing Live Object Set took {} seconds",
        start_time.elapsed().as_secs()
    );

    Ok(())
}

fn live_object_set_index_task<T: LiveObjectIndexer>(
    task_id: u8,
    bits: u8,
    authority_store: &AuthorityStore,
    mut object_indexer: T,
) -> Result<(), StorageError> {
    let mut id_bytes = [0; ObjectID::LENGTH];
    id_bytes[0] = task_id << (8 - bits);
    let start_id = ObjectID::new(id_bytes);

    id_bytes[0] |= (1 << (8 - bits)) - 1;
    for element in id_bytes.iter_mut().skip(1) {
        *element = u8::MAX;
    }
    let end_id = ObjectID::new(id_bytes);

    let mut object_scanned: u64 = 0;
    for object in authority_store
        .perpetual_tables
        .range_iter_live_object_set(Some(start_id), Some(end_id), false)
        .filter_map(LiveObject::to_normal)
    {
        object_scanned += 1;
        if object_scanned % 2_000_000 == 0 {
            info!(
                "[Index] Task {}: object scanned: {}",
                task_id, object_scanned
            );
        }

        object_indexer.index_object(object)?
    }

    object_indexer.finish()?;

    Ok(())
}
