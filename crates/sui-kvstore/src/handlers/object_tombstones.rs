// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::storage::ObjectKey;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::object_status::removed_objects;
use crate::tables;

/// Pipeline that writes a tombstone to the `objects` table at `(object_id, version)` for every
/// object a transaction deletes or wraps, so version-bounded reads stop at the removal instead
/// of returning the object's last live version. Live rows are written by
/// [`crate::ObjectsPipeline`].
pub struct ObjectTombstonesPipeline;

#[async_trait::async_trait]
impl Processor for ObjectTombstonesPipeline {
    const NAME: &'static str = "kvstore_object_tombstones";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let timestamp_ms = checkpoint.summary.timestamp_ms;
        let mut entries = vec![];
        for txn in &checkpoint.transactions {
            for (id, version, status) in removed_objects(&txn.effects) {
                entries.push(tables::make_entry(
                    tables::objects::encode_key(&ObjectKey(id, version)),
                    tables::objects::encode_tombstone(status)?,
                    Some(timestamp_ms),
                ));
            }
        }
        Ok(entries)
    }
}

impl BigTableProcessor for ObjectTombstonesPipeline {
    const TABLE: &'static str = tables::objects::NAME;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sui_types::base_types::SequenceNumber;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::object::Object;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::CheckpointSpan;
    use crate::KeyValueStoreReader;
    use crate::ObjectsPipeline;
    use crate::bigtable::client::BigTableClient;
    use crate::bigtable::mock_server::MockBigtableServer;

    /// An object wrapped and unwrapped by different transactions of one checkpoint: the
    /// checkpoint's final state shows it live, but reads bounded to a version between the wrap
    /// and the unwrap must not find it.
    #[tokio::test]
    async fn wrap_then_unwrap_in_one_checkpoint_hides_object_between_them() {
        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let cp0 = Arc::new(builder.build_checkpoint());
        let cp1 = Arc::new(
            builder
                .start_transaction(0)
                .transfer_object(0, 1)
                .finish_transaction()
                .start_transaction(0)
                .wrap_object(0)
                .finish_transaction()
                .start_transaction(0)
                .unwrap_object(0)
                .finish_transaction()
                .build_checkpoint(),
        );
        let id = TestCheckpointBuilder::derive_object_id(0);
        let [created, transferred, wrapped, unwrapped] = [
            &cp0.transactions[0],
            &cp1.transactions[0],
            &cp1.transactions[1],
            &cp1.transactions[2],
        ]
        .map(|txn| txn.effects.lamport_version());
        assert!(created < transferred && transferred < wrapped && wrapped < unwrapped);

        let mock = MockBigtableServer::new();
        let (addr, _handle) = mock.start().await.unwrap();
        let mut client =
            BigTableClient::new_for_host(addr.to_string(), "test".to_string(), "test", false)
                .await
                .unwrap();
        for checkpoint in [&cp0, &cp1] {
            let mut entries = ObjectsPipeline.process(checkpoint).await.unwrap();
            entries.extend(ObjectTombstonesPipeline.process(checkpoint).await.unwrap());
            client
                .write_entries(
                    tables::objects::NAME,
                    entries,
                    CheckpointSpan::single(checkpoint.summary.sequence_number),
                )
                .await
                .unwrap();
        }

        // The object's state at each version that changed it; `None` once wrapped.
        let changes: BTreeMap<SequenceNumber, Option<SequenceNumber>> = [
            (created, Some(created)),
            (transferred, Some(transferred)),
            (wrapped, None),
            (unwrapped, Some(unwrapped)),
        ]
        .into();
        let version_of = |object: Option<Object>| object.map(|o| (o.id(), o.version()));
        for version in 0..=unwrapped.value() + 1 {
            let version = SequenceNumber::from_u64(version);
            let expected = changes
                .range(..=version)
                .next_back()
                .and_then(|(_, live)| live.map(|live| (id, live)));
            let found = client
                .get_object_lt_or_eq_version(id, version)
                .await
                .unwrap();
            assert_eq!(version_of(found), expected, "at version {version}");
        }

        let latest = client.get_latest_object(&id).await.unwrap();
        assert_eq!(version_of(latest), Some((id, unwrapped)));
        assert!(
            client
                .get_objects(&[ObjectKey(id, wrapped)])
                .await
                .unwrap()
                .is_empty(),
            "the tombstone is not an object",
        );

        let live_versions: Vec<_> = client
            .get_live_object_versions(id, SequenceNumber::MIN..=SequenceNumber::MAX, 10, false)
            .await
            .unwrap()
            .into_iter()
            .map(|o| o.version())
            .collect();
        assert_eq!(live_versions, vec![created, transferred, unwrapped]);
    }
}
