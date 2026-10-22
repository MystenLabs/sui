// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::object_status::ObjectStatus;
use crate::object_status::removed_objects;
use crate::tables;

/// Pipeline that writes, for every object a checkpoint changes, the object's version and status
/// at the end of that checkpoint to the `object_versions_by_checkpoint` table.
pub struct ObjectVersionsByCheckpointPipeline;

#[async_trait::async_trait]
impl Processor for ObjectVersionsByCheckpointPipeline {
    const NAME: &'static str = "kvstore_object_versions_by_checkpoint";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.summary.sequence_number;
        let timestamp_ms = checkpoint.summary.timestamp_ms;

        // Transactions in a checkpoint are in causal order, so the last transaction to touch an
        // object determines its state at the end of the checkpoint.
        let mut final_states: HashMap<ObjectID, (SequenceNumber, ObjectStatus)> = HashMap::new();
        for txn in &checkpoint.transactions {
            for change in txn.effects.object_changes() {
                if let Some(version) = change.output_version {
                    final_states.insert(change.id, (version, ObjectStatus::Live));
                }
            }
            for (id, version, status) in removed_objects(&txn.effects) {
                final_states.insert(id, (version, status));
            }
        }

        Ok(final_states
            .into_iter()
            .map(|(id, (version, status))| {
                tables::make_entry(
                    tables::object_versions_by_checkpoint::encode_key(&id, cp_sequence_number),
                    tables::object_versions_by_checkpoint::encode(version, status),
                    Some(timestamp_ms),
                )
            })
            .collect())
    }
}

impl BigTableProcessor for ObjectVersionsByCheckpointPipeline {
    const TABLE: &'static str = tables::object_versions_by_checkpoint::NAME;
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::bigtable::proto::bigtable::v2::mutation;

    /// Decode every entry back into `(object_id, checkpoint) -> (version, status)`.
    fn decode_entries(
        entries: Vec<Entry>,
    ) -> HashMap<(ObjectID, u64), (SequenceNumber, ObjectStatus)> {
        entries
            .into_iter()
            .map(|entry| {
                let key = entry.row_key.as_ref();
                let object_id = ObjectID::from_bytes(&key[..ObjectID::LENGTH]).unwrap();
                let cp = u64::from_be_bytes(key[ObjectID::LENGTH..].try_into().unwrap());
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
                let value = tables::object_versions_by_checkpoint::decode(&cells).unwrap();
                ((object_id, cp), value)
            })
            .collect()
    }

    /// Each object a checkpoint touches gets exactly one row holding its state after the
    /// checkpoint's last transaction, even when several transactions change it.
    #[tokio::test]
    async fn records_final_state_of_each_changed_object() {
        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .create_owned_object(2)
            .create_owned_object(3)
            .finish_transaction();
        builder.build_checkpoint();

        // Checkpoint 1, across two transactions:
        // - object 0 is mutated twice,
        // - object 1 is deleted,
        // - object 2 is mutated and then wrapped,
        // - object 3 is wrapped and then unwrapped.
        let cp1 = Arc::new(
            builder
                .start_transaction(0)
                .transfer_object(0, 1)
                .delete_object(1)
                .transfer_object(2, 1)
                .wrap_object(3)
                .finish_transaction()
                .start_transaction(1)
                .transfer_object(0, 2)
                .wrap_object(2)
                .unwrap_object(3)
                .finish_transaction()
                .build_checkpoint(),
        );
        let first_lamport = cp1.transactions[0].effects.lamport_version();
        let second_lamport = cp1.transactions[1].effects.lamport_version();
        assert!(first_lamport < second_lamport);

        let entries = ObjectVersionsByCheckpointPipeline
            .process(&cp1)
            .await
            .unwrap();
        let entry_count = entries.len();
        let rows = decode_entries(entries);
        assert_eq!(entry_count, rows.len(), "one row per object");

        let object = |idx| (TestCheckpointBuilder::derive_object_id(idx), 1);
        assert_eq!(rows[&object(0)], (second_lamport, ObjectStatus::Live));
        assert_eq!(rows[&object(1)], (first_lamport, ObjectStatus::Deleted));
        assert_eq!(rows[&object(2)], (second_lamport, ObjectStatus::Wrapped));
        assert_eq!(rows[&object(3)], (second_lamport, ObjectStatus::Live));
    }
}
