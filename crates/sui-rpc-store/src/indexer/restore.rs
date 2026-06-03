// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Entry point for bulk-loading the [`RpcStoreSchema`]'s
//! derived-index CFs from a [`RestoreSource`].
//!
//! Registers the five live-object-derivable pipelines
//! ([`LiveObjects`], [`ObjectByOwner`], [`ObjectByType`],
//! [`Balance`], [`PackageVersions`]) against a single
//! [`RestoreDriver`] and returns a [`Service`] driving the
//! restore through to completion. Once finished, every pipeline's
//! `__restore` row is `Complete` and its `__watermark` row is set
//! to the source's target, so the regular
//! [`Indexer::add_pipelines`] path will accept them for tip
//! indexing.
//!
//! Restoration is run separately from tip indexing — open the
//! database, call [`restore_indexes`] to populate the indexes,
//! then construct an [`Indexer`] over the same store to start
//! tip-following.
//!
//! [`Indexer`]: crate::Indexer
//! [`Indexer::add_pipelines`]: crate::Indexer::add_pipelines

use std::sync::Arc;

use sui_consistent_store::Db;
use sui_consistent_store::restore::RestoreDriver;
use sui_consistent_store::restore::RestoreDriverConfig;
use sui_consistent_store::restore::RestoreSource;
use sui_futures::service::Service;

use crate::RpcStoreSchema;
use crate::indexer::balance::Balance;
use crate::indexer::live_objects::LiveObjects;
use crate::indexer::object_by_owner::ObjectByOwner;
use crate::indexer::object_by_type::ObjectByType;
use crate::indexer::package_versions::PackageVersions;

/// Register every [`Restore`]-implementing pipeline on a
/// [`RestoreDriver`] bound to `db` / `schema` and `source`, then
/// run the resulting [`Service`].
///
/// The returned `Service`'s primary task completes once every
/// pipeline transitions to [`RestoreState::Complete`].
///
/// [`Restore`]: sui_consistent_store::Restore
/// [`RestoreState::Complete`]: sui_consistent_store::restore_state::Complete
pub fn restore_indexes<Src: RestoreSource>(
    db: Db,
    schema: Arc<RpcStoreSchema>,
    source: Src,
    config: RestoreDriverConfig,
) -> anyhow::Result<Service> {
    let mut driver = RestoreDriver::new(db, schema, source, config);
    driver.register(LiveObjects)?;
    driver.register(ObjectByOwner)?;
    driver.register(ObjectByType)?;
    driver.register(Balance)?;
    driver.register(PackageVersions)?;
    driver.run()
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures::StreamExt;
    use futures::stream;
    use futures::stream::BoxStream;
    use sui_consistent_store::ChainId;
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_consistent_store::PipelineTaskKey;
    use sui_consistent_store::Watermark;
    use sui_consistent_store::restore::RestoreChunk;
    use sui_consistent_store::restore_state;
    use sui_indexer_alt_framework::pipeline::Processor;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::object::Object;

    use super::*;
    use crate::RpcStoreSchema;
    use crate::schema::object_by_owner::OwnerKind;

    /// Minimal [`RestoreSource`] that wraps a `Vec<RestoreChunk>`
    /// and uses the 4-byte BE chunk index as cursor. Lets us
    /// drive the end-to-end pipeline registration / commit path
    /// without standing up a real snapshot.
    struct VecSource {
        target: u64,
        chain_id: ChainId,
        chunks: Vec<RestoreChunk>,
    }

    impl VecSource {
        fn from_objects(target: u64, chain_id: ChainId, objects: Vec<Vec<Object>>) -> Self {
            let chunks = objects
                .into_iter()
                .enumerate()
                .map(|(i, objs)| RestoreChunk {
                    objects: objs,
                    cursor: Bytes::copy_from_slice(&(i as u32).to_be_bytes()),
                })
                .collect();
            Self {
                target,
                chain_id,
                chunks,
            }
        }
    }

    #[async_trait]
    impl RestoreSource for VecSource {
        fn target_checkpoint(&self) -> u64 {
            self.target
        }

        fn target_chain_id(&self) -> ChainId {
            self.chain_id
        }

        fn shards(&self) -> u32 {
            1
        }

        fn stream(
            &self,
            shard_id: u32,
            cursor: Option<Bytes>,
        ) -> BoxStream<'_, anyhow::Result<RestoreChunk>> {
            assert_eq!(shard_id, 0);
            let resume_after = cursor.map(|c| {
                let mut buf = [0u8; 4];
                buf.copy_from_slice(&c[..4]);
                u32::from_be_bytes(buf)
            });
            let chunks: Vec<_> = self
                .chunks
                .iter()
                .enumerate()
                .filter_map(|(i, chunk)| {
                    let i = i as u32;
                    if let Some(after) = resume_after
                        && i <= after
                    {
                        None
                    } else {
                        Some(Ok(RestoreChunk {
                            objects: chunk.objects.clone(),
                            cursor: chunk.cursor.clone(),
                        }))
                    }
                })
                .collect();
            stream::iter(chunks).boxed()
        }
    }

    /// End-to-end: drive a handful of address-owned objects
    /// through every registered pipeline. Verifies that the
    /// rows we expect end up in `live_objects` and
    /// `object_by_owner`, and that every pipeline's
    /// `__restore` / `__watermark` rows are set up for the
    /// tip-indexer to take over.
    #[tokio::test]
    async fn restore_indexes_populates_schema_and_finalises() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let schema = Arc::new(schema);

        let owner = SuiAddress::random_for_testing_only();
        let objects: Vec<Object> = (1..=4u8)
            .map(|i| Object::with_id_owner_for_testing(ObjectID::from_single_byte(i), owner))
            .collect();

        let chain_id = ChainId([7u8; 32]);
        let source = VecSource::from_objects(123, chain_id, vec![objects.clone()]);

        restore_indexes(
            db.clone(),
            schema.clone(),
            source,
            RestoreDriverConfig::default(),
        )
        .unwrap()
        .shutdown()
        .await
        .unwrap();

        // Each object's live pointer landed.
        for o in &objects {
            assert_eq!(
                schema.get_live_object_version(o.id()).unwrap(),
                Some(o.version()),
            );
        }

        // Owner index has every object under the same
        // AddressOwner(owner) key.
        let owned: Vec<(OwnerKind, ObjectID)> = schema
            .iter_objects_owned_by_address(owner)
            .unwrap()
            .map(Result::unwrap)
            .map(|(key, _v)| (key.kind, key.object_id))
            .collect();
        let mut got_ids: Vec<_> = owned.iter().map(|(_, id)| *id).collect();
        got_ids.sort();
        let mut expected_ids: Vec<_> = objects.iter().map(|o| o.id()).collect();
        expected_ids.sort();
        assert_eq!(got_ids, expected_ids);
        for (kind, _) in &owned {
            assert!(matches!(kind, OwnerKind::AddressOwner(addr) if *addr == owner));
        }

        // Every pipeline finished and has __restore Complete,
        // __watermark, and __chain_id all set.
        for name in [
            LiveObjects::NAME,
            ObjectByOwner::NAME,
            ObjectByType::NAME,
            Balance::NAME,
            PackageVersions::NAME,
        ] {
            let key = PipelineTaskKey::new(name);
            let state = db.framework().restore.get(&key).unwrap().unwrap();
            match state.state.unwrap() {
                restore_state::State::Complete(c) => assert_eq!(c.restored_at, 123),
                other => panic!("expected Complete, got {other:?}"),
            }
            let wm = db.framework().watermarks.get(&key).unwrap().unwrap();
            assert_eq!(wm, Watermark::for_checkpoint(123));
            let pinned_chain_id = db.framework().chain_ids.get(&key).unwrap().unwrap();
            assert_eq!(pinned_chain_id, chain_id);
        }
    }
}
