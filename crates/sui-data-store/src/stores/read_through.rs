// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    CheckpointContextStore, CheckpointExecutionContext, CheckpointObjectRequest,
    CheckpointObjectStore, EpochData, EpochStore, ObjectKey, ObjectStore, ReadDataStore,
    ReadWriteDataStore, SetupStore, StoreSummary, TransactionInfo, TransactionStore,
};
use anyhow::{Error, Result};
use mysten_common::ZipDebugEqIteratorExt;
use sui_types::{object::Object, supported_protocol_versions::ProtocolConfig};

/// Read-through caching for legacy APIs over a primary cache and secondary source.
/// Checkpoint APIs delegate to the secondary; checkpoint object answers are not cached here.
pub struct ReadThroughStore<P, S>
where
    P: ReadWriteDataStore,
    S: ReadDataStore,
{
    /// Primary store - supports both read and write (acts as cache)
    primary: P,
    /// Secondary store - read-only (acts as source of truth)
    secondary: S,
}

impl<P, S> ReadThroughStore<P, S>
where
    P: ReadWriteDataStore,
    S: ReadDataStore,
{
    /// Create a new read-through store with primary (cache) and secondary (source) stores
    pub fn new(primary: P, secondary: S) -> Self {
        Self { primary, secondary }
    }
}

impl<P, S> TransactionStore for ReadThroughStore<P, S>
where
    P: ReadWriteDataStore,
    S: ReadDataStore,
{
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        match self.primary.transaction_data_and_effects(tx_digest)? {
            Some(transaction_info) => Ok(Some(transaction_info)),
            None => self
                .secondary
                .transaction_data_and_effects(tx_digest)?
                .map_or(Ok(None), |info| {
                    self.primary.write_transaction(tx_digest, info.clone())?;
                    Ok(Some(info))
                }),
        }
    }
}

impl<P, S> EpochStore for ReadThroughStore<P, S>
where
    P: ReadWriteDataStore,
    S: ReadDataStore,
{
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error> {
        match self.primary.epoch_info(epoch)? {
            Some(epoch_data) => Ok(Some(epoch_data)),
            None => match self.secondary.epoch_info(epoch)? {
                Some(epoch_data) => {
                    self.primary.write_epoch_info(epoch, epoch_data.clone())?;
                    Ok(Some(epoch_data))
                }
                None => Ok(None),
            },
        }
    }

    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        match self.primary.protocol_config(epoch)? {
            Some(config) => Ok(Some(config)),
            None => self.secondary.protocol_config(epoch),
        }
    }
}

/// Delegates checkpoint selection to the secondary store and caches its epoch metadata.
impl<P, S> CheckpointContextStore for ReadThroughStore<P, S>
where
    P: ReadWriteDataStore,
    S: ReadDataStore + CheckpointContextStore,
{
    fn checkpoint_execution_context(
        &self,
        checkpoint: Option<u64>,
    ) -> Result<CheckpointExecutionContext, Error> {
        let context = self.secondary.checkpoint_execution_context(checkpoint)?;
        self.primary
            .write_epoch_info(context.epoch.epoch_id, context.epoch.clone())?;
        Ok(context)
    }
}

/// Delegates checkpoint-bound reads because legacy cache keys cannot preserve their semantics.
impl<P, S> CheckpointObjectStore for ReadThroughStore<P, S>
where
    P: ReadWriteDataStore,
    S: ReadDataStore + CheckpointObjectStore,
{
    fn get_checkpoint_objects(
        &self,
        context: &CheckpointExecutionContext,
        requests: &[CheckpointObjectRequest],
    ) -> Result<Vec<Option<Object>>, Error> {
        self.secondary.get_checkpoint_objects(context, requests)
    }
}

impl<P, S> ObjectStore for ReadThroughStore<P, S>
where
    P: ReadWriteDataStore,
    S: ReadDataStore,
{
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let cached_objects = self.primary.get_objects(keys)?;

        let mut keys_to_fetch = Vec::new();
        let mut none_object_idx = Vec::new();
        for (i, object) in cached_objects.iter().enumerate() {
            if object.is_none() {
                keys_to_fetch.push(keys[i].clone());
                none_object_idx.push(i);
            }
        }

        let mut objects = cached_objects;
        if !keys_to_fetch.is_empty() {
            let fetched_objects = self.secondary.get_objects(&keys_to_fetch)?;

            // Sanity checks: the three vectors must align one-to-one
            assert_eq!(none_object_idx.len(), keys_to_fetch.len());
            assert_eq!(fetched_objects.len(), keys_to_fetch.len());

            for ((idx, key), fetched) in none_object_idx
                .iter()
                .zip_debug_eq(keys_to_fetch.iter())
                .zip_debug_eq(fetched_objects.iter())
            {
                // REVIEW: should we cache `None` to avoid repeated misses? Doing so would
                // require API changes to represent cached-miss entries.
                if let Some((object, actual_version)) = fetched {
                    self.primary
                        .write_object(key, object.clone(), *actual_version)?;
                    objects[*idx] = Some((object.clone(), *actual_version));
                }
            }
        }
        Ok(objects)
    }
}

impl<P, S> StoreSummary for ReadThroughStore<P, S>
where
    P: ReadWriteDataStore + StoreSummary,
    S: ReadDataStore + StoreSummary,
{
    fn summary<W: std::io::Write>(&self, w: &mut W) -> Result<()> {
        self.primary.summary(w)?;
        self.secondary.summary(w)
    }
}

impl<P, S> SetupStore for ReadThroughStore<P, S>
where
    P: ReadWriteDataStore + SetupStore,
    S: ReadDataStore + SetupStore,
{
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        // Call setup on secondary and pass the return value to primary if not an error
        let chain_id = self.secondary.setup(None)?;
        if let Some(ref chain_id) = chain_id {
            self.primary.setup(Some(chain_id.clone()))?;
        }
        Ok(chain_id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CheckpointExecutionContext, EpochData, stores::InMemoryStore};
    use std::sync::atomic::{AtomicU64, Ordering};
    use sui_types::{
        digests::{ChainIdentifier, CheckpointDigest},
        object::Object,
    };

    struct ContextSource {
        context: CheckpointExecutionContext,
        calls: AtomicU64,
        object_calls: AtomicU64,
    }

    impl TransactionStore for ContextSource {
        fn transaction_data_and_effects(
            &self,
            _tx_digest: &str,
        ) -> Result<Option<TransactionInfo>, Error> {
            Ok(None)
        }
    }

    impl EpochStore for ContextSource {
        fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
            Ok(None)
        }

        fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
            Ok(None)
        }
    }

    impl ObjectStore for ContextSource {
        fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
            Ok(keys.iter().map(|_| None).collect())
        }
    }

    impl CheckpointContextStore for ContextSource {
        fn checkpoint_execution_context(
            &self,
            _checkpoint: Option<u64>,
        ) -> Result<CheckpointExecutionContext, Error> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(self.context.clone())
        }
    }

    impl CheckpointObjectStore for ContextSource {
        fn get_checkpoint_objects(
            &self,
            context: &CheckpointExecutionContext,
            requests: &[CheckpointObjectRequest],
        ) -> Result<Vec<Option<Object>>, Error> {
            assert_eq!(context, &self.context);
            self.object_calls.fetch_add(1, Ordering::Relaxed);
            Ok(requests.iter().map(|_| None).collect())
        }
    }

    #[test]
    fn checkpoint_context_comes_from_secondary_and_caches_epoch_data() {
        let epoch = EpochData {
            epoch_id: 7,
            protocol_version: 42,
            rgp: 1_000,
            start_timestamp: 123,
        };
        let primary = InMemoryStore::new(crate::Node::Mainnet);
        primary.add_epoch_data(
            epoch.epoch_id,
            EpochData {
                protocol_version: 1,
                ..epoch.clone()
            },
        );
        let source = ContextSource {
            context: CheckpointExecutionContext {
                chain_identifier: ChainIdentifier::random(),
                checkpoint: 10,
                checkpoint_digest: CheckpointDigest::random(),
                epoch: epoch.clone(),
            },
            calls: AtomicU64::new(0),
            object_calls: AtomicU64::new(0),
        };
        let store = ReadThroughStore::new(primary, source);

        let context = store.checkpoint_execution_context(None).unwrap();

        assert_eq!(context.epoch, epoch);
        assert_eq!(store.secondary.calls.load(Ordering::Relaxed), 1);
        assert_eq!(store.primary.epoch_info(7).unwrap(), Some(epoch));
    }

    /// A cached object body does not establish that its version was visible at the requested
    /// checkpoint, so checkpoint-bound reads must bypass legacy cache entries.
    #[test]
    fn checkpoint_objects_delegate_without_using_legacy_cache_entries() {
        let context = CheckpointExecutionContext {
            chain_identifier: ChainIdentifier::random(),
            checkpoint: 10,
            checkpoint_digest: CheckpointDigest::random(),
            epoch: EpochData {
                epoch_id: 7,
                protocol_version: 42,
                rgp: 1_000,
                start_timestamp: 123,
            },
        };
        let object = Object::with_owner_for_testing(
            sui_types::base_types::SuiAddress::random_for_testing_only(),
        );
        let primary = InMemoryStore::new(crate::Node::Mainnet);
        primary.add_object_data(object.id(), object.version().value(), object.clone());
        let source = ContextSource {
            context: context.clone(),
            calls: AtomicU64::new(0),
            object_calls: AtomicU64::new(0),
        };
        let store = ReadThroughStore::new(primary, source);
        let requests = [CheckpointObjectRequest {
            object_id: object.id(),
            selector: crate::CheckpointObjectSelector::ExactVersion(object.version().value()),
        }];

        let results = store.get_checkpoint_objects(&context, &requests).unwrap();

        assert_eq!(results, vec![None]);
        assert_eq!(store.secondary.object_calls.load(Ordering::Relaxed), 1);
    }
}
