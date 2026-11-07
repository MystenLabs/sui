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
use crate::tables::ObjectSchema;

/// Pipeline that writes the [`ObjectSchema::V2`] `objects_v2` table: every live object version,
/// and a tombstone at `(object_id, version)` for every object a transaction deletes or wraps, so
/// version-bounded reads stop at the removal instead of returning the object's last live version.
///
/// A live row and a tombstone never share a key, because a transaction that deletes or wraps an
/// object does not also output it. So writes are idempotent and can land in any order.
pub struct ObjectsV2Pipeline;

#[async_trait::async_trait]
impl Processor for ObjectsV2Pipeline {
    const NAME: &'static str = "kvstore_objects_v2";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let timestamp_ms = checkpoint.summary.timestamp_ms;
        let mut entries = vec![];
        for txn in &checkpoint.transactions {
            for object in txn.output_objects(&checkpoint.object_set) {
                entries.push(tables::make_entry(
                    tables::objects::encode_key(
                        ObjectSchema::V2,
                        &ObjectKey(object.id(), object.version()),
                    ),
                    tables::objects::encode(object)?,
                    Some(timestamp_ms),
                ));
            }
            for (id, version, status) in removed_objects(&txn.effects) {
                entries.push(tables::make_entry(
                    tables::objects::encode_key(ObjectSchema::V2, &ObjectKey(id, version)),
                    tables::objects::encode_tombstone(status)?,
                    Some(timestamp_ms),
                ));
            }
        }
        Ok(entries)
    }
}

impl BigTableProcessor for ObjectsV2Pipeline {
    const TABLE: &'static str = tables::objects::NAME_V2;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::HashSet;

    use bytes::Bytes;
    use sui_types::base_types::SequenceNumber;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::object::Object;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::CheckpointSpan;
    use crate::KeyValueStoreReader;
    use crate::ObjectStatus;
    use crate::bigtable::client::BigTableClient;
    use crate::bigtable::mock_server::MockBigtableServer;
    use crate::bigtable::proto::bigtable::v2::mutation;
    use crate::tables::objects::ObjectRow;

    /// Decode every entry back into its key and row.
    fn decode_entries(entries: Vec<Entry>) -> Vec<(ObjectKey, ObjectRow)> {
        entries
            .into_iter()
            .map(|entry| {
                let key =
                    tables::objects::decode_key(ObjectSchema::V2, entry.row_key.as_ref()).unwrap();
                let cells: Vec<(Bytes, Bytes)> = entry
                    .mutations
                    .into_iter()
                    .map(|m| match m.mutation {
                        Some(mutation::Mutation::SetCell(cell)) => {
                            (cell.column_qualifier, cell.value)
                        }
                        other => panic!("unexpected mutation {other:?}"),
                    })
                    .collect();
                (key, tables::objects::decode(&cells).unwrap())
            })
            .collect()
    }

    /// One checkpoint writes a live row for every output version and a tombstone for every
    /// deletion and wrap, each at its own key.
    #[tokio::test]
    async fn writes_live_rows_and_tombstones_at_distinct_keys() {
        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .create_owned_object(2)
            .finish_transaction();
        builder.build_checkpoint();
        let cp1 = Arc::new(
            builder
                .start_transaction(0)
                .transfer_object(0, 1)
                .delete_object(1)
                .wrap_object(2)
                .finish_transaction()
                .build_checkpoint(),
        );
        let lamport = cp1.transactions[0].effects.lamport_version();

        let rows = decode_entries(ObjectsV2Pipeline.process(&cp1).await.unwrap());
        let keys: HashSet<_> = rows.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys.len(), rows.len(), "a key was written twice");

        let object = |idx| ObjectKey(TestCheckpointBuilder::derive_object_id(idx), lamport);
        let row = |key: ObjectKey| rows.iter().find(|(k, _)| *k == key).map(|(_, row)| row);
        assert!(matches!(row(object(0)), Some(ObjectRow::Live(o)) if o.version() == lamport));
        assert_eq!(
            row(object(1)),
            Some(&ObjectRow::Tombstone(ObjectStatus::Deleted))
        );
        assert_eq!(
            row(object(2)),
            Some(&ObjectRow::Tombstone(ObjectStatus::Wrapped))
        );
    }

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
                .unwrap()
                .with_object_schema(ObjectSchema::V2);
        for checkpoint in [&cp0, &cp1] {
            client
                .write_entries(
                    tables::objects::NAME_V2,
                    ObjectsV2Pipeline.process(checkpoint).await.unwrap(),
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

        for (descending, expected) in [
            (false, vec![created, transferred, unwrapped]),
            (true, vec![unwrapped, transferred, created]),
        ] {
            let live_versions: Vec<_> = client
                .get_live_object_versions(
                    id,
                    SequenceNumber::MIN..=SequenceNumber::MAX,
                    10,
                    descending,
                )
                .await
                .unwrap()
                .into_iter()
                .map(|o| o.version())
                .collect();
            assert_eq!(live_versions, expected, "descending: {descending}");
        }
    }
}
