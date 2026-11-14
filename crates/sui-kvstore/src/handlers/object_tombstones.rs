// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::storage::ObjectKey;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::object_status::removed_objects;
use crate::tables;

/// Pipeline that writes a tombstone to the `objects` table at `(object_id, version)` for every
/// object a transaction deletes or wraps, so version-bounded reads stop at the removal instead
/// of returning the object's last live version. Each tombstone records the removing transaction's
/// digest. Live rows are written by [`crate::ObjectsPipeline`].
pub struct ObjectTombstonesPipeline;

#[async_trait::async_trait]
impl Processor for ObjectTombstonesPipeline {
    const NAME: &'static str = "kvstore_object_tombstones";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let timestamp_ms = checkpoint.summary.timestamp_ms;
        let mut entries = vec![];
        for txn in &checkpoint.transactions {
            let transaction = txn.effects.transaction_digest();
            for (id, version, status) in removed_objects(&txn.effects) {
                entries.push(tables::make_entry(
                    tables::objects::encode_key(&ObjectKey(id, version)),
                    tables::objects::encode_tombstone(status, transaction)?,
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
    use std::collections::HashMap;

    use bytes::Bytes;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SequenceNumber;
    use sui_types::base_types::SuiAddress;
    use sui_types::base_types::random_object_ref;
    use sui_types::digests::ObjectDigest;
    use sui_types::digests::TransactionDigest;
    use sui_types::effects::EffectsObjectChange;
    use sui_types::effects::TransactionEffects;
    use sui_types::execution_status::ExecutionStatus;
    use sui_types::gas::GasCostSummary;
    use sui_types::object::Object;
    use sui_types::object::Owner;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::CheckpointSpan;
    use crate::KeyValueStoreReader;
    use crate::ObjectStatus;
    use crate::ObjectsPipeline;
    use crate::bigtable::client::BigTableClient;
    use crate::bigtable::mock_server::MockBigtableServer;
    use crate::bigtable::proto::bigtable::v2::mutation;
    use crate::tables::objects::ObjectRow;
    use crate::tables::objects::ObjectTombstone;

    /// Every tombstone records the digest of the transaction that removed the object, for each
    /// kind of removal, from both V1 and V2 effects.
    #[tokio::test]
    async fn tombstones_record_removing_transaction() {
        let removal_version = SequenceNumber::from_u64(10);
        let [deleted, unwrapped_then_deleted, wrapped] = [0, 1, 2].map(|_| ObjectID::random());
        let v1_digest = TransactionDigest::random();
        let v1 = TransactionEffects::new_from_execution_v1(
            ExecutionStatus::Success,
            0,
            GasCostSummary::default(),
            vec![],
            vec![],
            v1_digest,
            vec![],
            vec![],
            vec![],
            vec![(
                deleted,
                removal_version,
                ObjectDigest::OBJECT_DIGEST_DELETED,
            )],
            vec![(
                unwrapped_then_deleted,
                removal_version,
                ObjectDigest::OBJECT_DIGEST_DELETED,
            )],
            vec![(
                wrapped,
                removal_version,
                ObjectDigest::OBJECT_DIGEST_WRAPPED,
            )],
            (random_object_ref(), Owner::AddressOwner(SuiAddress::ZERO)),
            None,
            vec![],
        );

        let [v2_deleted, v2_unwrapped_then_deleted, v2_wrapped] =
            [0, 1, 2].map(|_| ObjectID::random());
        let v2_digest = TransactionDigest::random();
        let existed = Some((
            (SequenceNumber::from_u64(1), ObjectDigest::random()),
            Owner::AddressOwner(SuiAddress::ZERO),
        ));
        let v2 = TransactionEffects::new_from_execution_v2(
            ExecutionStatus::Success,
            0,
            GasCostSummary::default(),
            vec![],
            v2_digest,
            removal_version,
            BTreeMap::from([
                (
                    v2_deleted,
                    EffectsObjectChange::new(existed.clone(), None, false, true),
                ),
                (
                    v2_unwrapped_then_deleted,
                    EffectsObjectChange::new(None, None, false, true),
                ),
                (
                    v2_wrapped,
                    EffectsObjectChange::new(existed, None, false, false),
                ),
            ]),
            None,
            None,
            vec![],
        );

        // Two transactions from the builder, with their effects replaced by the ones above.
        let mut checkpoint = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .finish_transaction()
            .start_transaction(1)
            .finish_transaction()
            .build_checkpoint();
        checkpoint.transactions[0].effects = v1;
        checkpoint.transactions[1].effects = v2;

        let entries = ObjectTombstonesPipeline
            .process(&Arc::new(checkpoint))
            .await
            .unwrap();
        assert_eq!(entries.len(), 6, "one entry per removed object");
        // Row key -> (decoded tombstone, value of the `t` cell).
        let rows: HashMap<Bytes, (ObjectRow, Option<Bytes>)> = entries
            .into_iter()
            .map(|entry| {
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
                let transaction = cells
                    .iter()
                    .find(|(column, _)| column == tables::objects::col::TRANSACTION.as_bytes())
                    .map(|(_, value)| value.clone());
                let row = tables::objects::decode(&cells).unwrap();
                (entry.row_key, (row, transaction))
            })
            .collect();

        let tombstone = |id, status, transaction: TransactionDigest| {
            (
                Bytes::from(tables::objects::encode_key(&ObjectKey(id, removal_version))),
                (
                    ObjectRow::Tombstone(ObjectTombstone {
                        status,
                        transaction: Some(transaction),
                    }),
                    Some(Bytes::copy_from_slice(transaction.inner())),
                ),
            )
        };
        let expected = HashMap::from([
            tombstone(deleted, ObjectStatus::Deleted, v1_digest),
            tombstone(unwrapped_then_deleted, ObjectStatus::Deleted, v1_digest),
            tombstone(wrapped, ObjectStatus::Wrapped, v1_digest),
            tombstone(v2_deleted, ObjectStatus::Deleted, v2_digest),
            tombstone(v2_unwrapped_then_deleted, ObjectStatus::Deleted, v2_digest),
            tombstone(v2_wrapped, ObjectStatus::Wrapped, v2_digest),
        ]);
        assert_eq!(rows, expected);
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
