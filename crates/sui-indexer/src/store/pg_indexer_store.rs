// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use cached::proc_macro::once;
use diesel::dsl::{count, max};
use diesel::pg::PgConnection;
use diesel::sql_types::{BigInt, VarChar};
use diesel::upsert::excluded;
use diesel::ExpressionMethods;
use diesel::{OptionalExtension, QueryableByName};
use diesel::{QueryDsl, RunQueryDsl};
use fastcrypto::hash::Digest;
use fastcrypto::traits::ToFromBytes;
use move_bytecode_utils::module_cache::SyncModuleCache;
use move_core_types::identifier::Identifier;
use prometheus::{Histogram, IntCounter};
use tracing::info;

use sui_json_rpc_types::{
    CheckpointId, EpochInfo, EventFilter, EventPage, MoveCallMetrics, MoveFunctionName,
    NetworkMetrics, SuiEvent, SuiObjectDataFilter,
};
use sui_json_rpc_types::{
    SuiTransactionBlock, SuiTransactionBlockEffects, SuiTransactionBlockEvents,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::committee::{EpochId, ProtocolVersion};
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::TransactionDigest;
use sui_types::event::EventID;
use sui_types::messages_checkpoint::{
    CheckpointCommitment, CheckpointSequenceNumber, ECMHLiveObjectSetDigest, EndOfEpochData,
};
use sui_types::object::ObjectRead;
use sui_types::transaction::SenderSignedData;

use crate::errors::{Context, IndexerError};
use crate::metrics::IndexerMetrics;
use crate::models::addresses::{ActiveAddress, Address, AddressStats, DBAddressStats};
use crate::models::checkpoint_metrics::{CheckpointMetrics, Tps};
use crate::models::checkpoints::Checkpoint;
use crate::models::epoch::DBEpochInfo;
use crate::models::events::Event;
use crate::models::network_metrics::{DBMoveCallMetrics, DBNetworkMetrics};
use crate::models::objects::{
    compose_object_bulk_insert_update_query, group_and_sort_objects, Object,
};
use crate::models::packages::Package;
use crate::models::system_state::DBValidatorSummary;
use crate::models::transaction_index::{ChangedObject, InputObject, MoveCall, Recipient};
use crate::models::transactions::Transaction;
use crate::schema::{
    active_addresses, address_stats, addresses, changed_objects, checkpoint_metrics, checkpoints,
    epochs, events, input_objects, move_calls, objects, objects_history, packages, recipients,
    system_states, transactions, validators,
};
use crate::store::diesel_marco::{read_only_blocking, transactional_blocking};
use crate::store::module_resolver::IndexerModuleResolver;
use crate::store::query::DBFilter;
use crate::store::TransactionObjectChanges;
use crate::store::{IndexerStore, TemporaryEpochStore};
use crate::PgConnectionPool;

const MAX_EVENT_PAGE_SIZE: usize = 1000;
const PG_COMMIT_CHUNK_SIZE: usize = 1000;

const GET_PARTITION_SQL: &str = r#"
SELECT parent.relname                           AS table_name,
       MAX(SUBSTRING(child.relname FROM '\d$')) AS last_partition
FROM pg_inherits
         JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
         JOIN pg_class child ON pg_inherits.inhrelid = child.oid
         JOIN pg_namespace nmsp_parent ON nmsp_parent.oid = parent.relnamespace
         JOIN pg_namespace nmsp_child ON nmsp_child.oid = child.relnamespace
WHERE parent.relkind = 'p'
GROUP BY table_name;
"#;

#[derive(QueryableByName, Debug, Clone)]
struct TempDigestTable {
    #[diesel(sql_type = VarChar)]
    digest_name: String,
}

#[derive(Clone)]
pub struct PgIndexerStore {
    blocking_cp: PgConnectionPool,
    // MUSTFIX(gegaowp): temporarily disable partition management.
    #[allow(dead_code)]
    partition_manager: PartitionManager,
    module_cache: Arc<SyncModuleCache<IndexerModuleResolver>>,
    metrics: IndexerMetrics,
}

impl PgIndexerStore {
    pub fn new(blocking_cp: PgConnectionPool, metrics: IndexerMetrics) -> Self {
        let module_cache = Arc::new(SyncModuleCache::new(IndexerModuleResolver::new(
            blocking_cp.clone(),
        )));
        PgIndexerStore {
            blocking_cp: blocking_cp.clone(),
            partition_manager: PartitionManager::new(blocking_cp).unwrap(),
            module_cache,
            metrics,
        }
    }

    pub fn get_sui_types_object(
        &self,
        object_id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<sui_types::object::Object, IndexerError> {
        let pg_object = read_only_blocking!(&self.blocking_cp, |conn| {
            objects_history::dsl::objects_history
                .select((
                    objects_history::epoch,
                    objects_history::checkpoint,
                    objects_history::object_id,
                    objects_history::version,
                    objects_history::object_digest,
                    objects_history::owner_type,
                    objects_history::owner_address,
                    objects_history::initial_shared_version,
                    objects_history::previous_transaction,
                    objects_history::object_type,
                    objects_history::object_status,
                    objects_history::has_public_transfer,
                    objects_history::storage_rebate,
                    objects_history::bcs,
                ))
                .filter(objects_history::object_id.eq(object_id.to_string()))
                .filter(objects_history::version.eq(version.value() as i64))
                // pick data from checkpoint if available
                .order(objects_history::checkpoint.desc())
                .first::<Object>(conn)
        })
        .context("Failed reading Object from PostgresDB");
        match pg_object {
            Ok(pg_object) => {
                let object = sui_types::object::Object::try_from(pg_object)?;
                Ok(object)
            }
            Err(e) => Err(e),
        }
    }

    pub fn find_sui_types_object_lt_or_eq_version(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Option<sui_types::object::Object>, IndexerError> {
        let pg_object = read_only_blocking!(&self.blocking_cp, |conn| {
            objects_history::dsl::objects_history
                .select((
                    objects_history::epoch,
                    objects_history::checkpoint,
                    objects_history::object_id,
                    objects_history::version,
                    objects_history::object_digest,
                    objects_history::owner_type,
                    objects_history::owner_address,
                    objects_history::initial_shared_version,
                    objects_history::previous_transaction,
                    objects_history::object_type,
                    objects_history::object_status,
                    objects_history::has_public_transfer,
                    objects_history::storage_rebate,
                    objects_history::bcs,
                ))
                .filter(objects_history::object_id.eq(id.to_string()))
                .filter(objects_history::version.le(version.value() as i64))
                .order_by(objects_history::version.desc())
                // pick data from checkpoint if available
                .order_by(objects_history::checkpoint.desc())
                .first::<Object>(conn)
                .optional()
        })
        .context("Failed reading Object before version from PostgresDB");
        match pg_object {
            Ok(Some(pg_object)) => {
                let object = sui_types::object::Object::try_from(pg_object)?;
                Ok(Some(object))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn get_latest_tx_checkpoint_sequence_number(&self) -> Result<i64, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints::dsl::checkpoints
                .select(max(checkpoints::sequence_number))
                .first::<Option<i64>>(conn)
                // -1 to differentiate between no checkpoints and the first checkpoint
                .map(|o| o.unwrap_or(-1))
        })
        .context("Failed reading latest checkpoint sequence number from PostgresDB")
    }

    fn get_latest_object_checkpoint_sequence_number(&self) -> Result<i64, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            objects::dsl::objects
                .select(max(objects::checkpoint))
                .first::<Option<i64>>(conn)
                // -1 to differentiate between no checkpoints and the first checkpoint
                .map(|o| o.unwrap_or(-1))
        })
        .context("Failed reading latest object checkpoint sequence number from PostgresDB")
    }

    fn get_checkpoint(
        &self,
        id: CheckpointId,
    ) -> Result<sui_json_rpc_types::Checkpoint, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let cp: Checkpoint = match id {
                CheckpointId::SequenceNumber(seq) => checkpoints::dsl::checkpoints
                    .filter(checkpoints::sequence_number.eq(seq as i64))
                    .limit(1)
                    .first(conn),
                CheckpointId::Digest(digest) => checkpoints::dsl::checkpoints
                    .filter(checkpoints::checkpoint_digest.eq(digest.base58_encode()))
                    .limit(1)
                    .first(conn),
            }?;
            let end_of_epoch_data = if cp.end_of_epoch {
                let (
                    next_version,
                    next_epoch_committee,
                    next_epoch_committee_stake,
                    epoch_commitments,
                ) = epochs::dsl::epochs
                    .select((
                        epochs::next_epoch_version,
                        epochs::next_epoch_committee,
                        epochs::next_epoch_committee_stake,
                        epochs::epoch_commitments,
                    ))
                    .filter(epochs::epoch.eq(cp.epoch))
                    .first::<(
                        Option<i64>,
                        Vec<Option<Vec<u8>>>,
                        Vec<Option<i64>>,
                        Vec<Option<Vec<u8>>>,
                    )>(conn)?;

                let next_epoch_committee = next_epoch_committee
                    .iter()
                    .flatten()
                    .zip(next_epoch_committee_stake.iter().flatten())
                    .map(|(name, vote)| {
                        AuthorityPublicKeyBytes::from_bytes(name.as_slice())
                            .map(|b| (b, (*vote) as u64))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let epoch_commitments = epoch_commitments
                    .iter()
                    .flatten()
                    .flat_map(|v| {
                        if let Ok(v) = v.clone().try_into() {
                            return Some(CheckpointCommitment::ECMHLiveObjectSetDigest(
                                ECMHLiveObjectSetDigest::from(Digest::new(v)),
                            ));
                        }
                        None
                    })
                    .collect::<Vec<_>>();

                next_version.map(|next_epoch_protocol_version| EndOfEpochData {
                    next_epoch_committee,
                    next_epoch_protocol_version: ProtocolVersion::from(
                        next_epoch_protocol_version as u64,
                    ),
                    epoch_commitments,
                })
            } else {
                None
            };
            cp.into_rpc(end_of_epoch_data)
        })
        .context(format!("Failed reading checkpoint {:?} from PostgresDB", id).as_str())
    }

    fn get_indexer_checkpoint(&self) -> Result<Checkpoint, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints::dsl::checkpoints
                .order_by(checkpoints::sequence_number.desc())
                .limit(1)
                .first(conn)
        })
        .context("Failed reading indexer checkpoint from PostgresDB")
    }

    fn get_checkpoints(
        &self,
        cursor: Option<CheckpointId>,
        limit: usize,
    ) -> Result<Vec<sui_json_rpc_types::Checkpoint>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let cp_vec: Vec<Checkpoint> = match cursor {
                Some(CheckpointId::SequenceNumber(seq)) => checkpoints::dsl::checkpoints
                    .filter(checkpoints::sequence_number.gt(seq as i64))
                    .order_by(checkpoints::sequence_number)
                    .limit(limit as i64)
                    .load::<Checkpoint>(conn)?,
                Some(CheckpointId::Digest(digest)) => Err(anyhow!(
                    "CheckpointId::Digest: {} is not supported as cursor in get_checkpoints.",
                    digest
                ))?,
                None => checkpoints::dsl::checkpoints
                    .order_by(checkpoints::sequence_number)
                    .limit(limit as i64)
                    .load::<Checkpoint>(conn)?,
            };
            cp_vec
                .into_iter()
                .map(|cp| cp.into_rpc(None))
                .collect::<Result<Vec<_>, _>>()
        })
        .context(
            format!(
                "Failed reading checkpoints with cursor: {:?} and limit: {} from PostgresDB",
                cursor, limit
            )
            .as_str(),
        )
    }

    fn get_indexer_checkpoints(
        &self,
        cursor: i64,
        limit: usize,
    ) -> Result<Vec<Checkpoint>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints::dsl::checkpoints
                .filter(checkpoints::sequence_number.gt(cursor))
                .order_by(checkpoints::sequence_number)
                .limit(limit as i64)
                .load::<Checkpoint>(conn)
        })
        .context(
            format!(
                "Failed reading checkpoints with cursor: {} and limit: {} from PostgresDB",
                cursor, limit
            )
            .as_str(),
        )
    }

    fn get_checkpoint_sequence_number(
        &self,
        digest: CheckpointDigest,
    ) -> Result<CheckpointSequenceNumber, IndexerError> {
        Ok(
            read_only_blocking!(&self.blocking_cp, |conn| checkpoints::dsl::checkpoints
                .select(checkpoints::sequence_number)
                .filter(checkpoints::checkpoint_digest.eq(digest.base58_encode()))
                .first::<i64>(conn))
            .context("Failed reading checkpoint seq number from PostgresDB")? as u64,
        )
    }

    fn get_event(&self, id: EventID) -> Result<Event, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| events::table
            .filter(events::dsl::transaction_digest.eq(id.tx_digest.base58_encode()))
            .filter(events::dsl::event_sequence.eq(id.event_seq as i64))
            .first::<Event>(conn))
        .context("Failed reading event from PostgresDB")
    }

    fn get_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> Result<EventPage, IndexerError> {
        let mut boxed_query = events::table.into_boxed();
        match query {
            EventFilter::All(..) => {}
            EventFilter::Transaction(digest) => {
                boxed_query =
                    boxed_query.filter(events::dsl::transaction_digest.eq(digest.base58_encode()));
            }
            EventFilter::MoveModule { package, module } => {
                boxed_query = boxed_query
                    .filter(events::dsl::package.eq(package.to_string()))
                    .filter(events::dsl::module.eq(module.to_string()));
            }
            EventFilter::MoveEventType(struct_name) => {
                boxed_query =
                    boxed_query.filter(events::dsl::event_type.eq(struct_name.to_string()));
            }
            EventFilter::Sender(sender) => {
                boxed_query = boxed_query.filter(events::dsl::sender.eq(sender.to_string()));
            }
            EventFilter::TimeRange {
                start_time,
                end_time,
            } => {
                boxed_query = boxed_query
                    .filter(events::dsl::event_time_ms.ge(start_time as i64))
                    .filter(events::dsl::event_time_ms.lt(end_time as i64));
            }
            // TODO: Implement EventFilter to SQL
            _ => {
                return Err(IndexerError::NotSupportedError(format!(
                    "Filter type [{query:?}] not supported by the Indexer."
                )))
            }
        }

        let mut page_limit = limit.unwrap_or(MAX_EVENT_PAGE_SIZE);
        if page_limit > MAX_EVENT_PAGE_SIZE {
            Err(IndexerError::InvalidArgumentError(format!(
                "Limit {} exceeds the maximum page size {}",
                page_limit, MAX_EVENT_PAGE_SIZE
            )))?;
        }
        // fetch one more item to tell if there is next page
        page_limit += 1;

        let pg_cursor =
            if let Some(cursor) = cursor {
                Some(self.get_event(cursor)?.id.ok_or_else(|| {
                    IndexerError::PostgresReadError("Event ID is None".to_string())
                })?)
            } else {
                None
            };

        let events_vec: Vec<Event> = read_only_blocking!(&self.blocking_cp, |conn| {
            if let Some(pg_cursor) = pg_cursor {
                if descending_order {
                    boxed_query = boxed_query.filter(events::dsl::id.lt(pg_cursor));
                } else {
                    boxed_query = boxed_query.filter(events::dsl::id.gt(pg_cursor));
                }
            }
            if descending_order {
                boxed_query = boxed_query.order(events::id.desc());
            } else {
                boxed_query = boxed_query.order(events::id.asc());
            }
            boxed_query.load(conn)
        })
        .context("Failed reading events from PostgresDB")?;

        let mut sui_event_vec = events_vec
            .into_iter()
            .map(|event| event.try_into(&self.module_cache))
            .collect::<Result<Vec<SuiEvent>, _>>()?;
        // reset to original limit for checking and truncating
        page_limit -= 1;
        let has_next_page = sui_event_vec.len() > page_limit;
        sui_event_vec.truncate(page_limit);
        let next_cursor = sui_event_vec.last().map(|e| e.id.clone());
        Ok(EventPage {
            data: sui_event_vec,
            next_cursor,
            has_next_page,
        })
    }

    fn get_total_transaction_number_from_checkpoints(&self) -> Result<i64, IndexerError> {
        let checkpoint: Checkpoint = read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints::dsl::checkpoints
                .order(checkpoints::dsl::network_total_transactions.desc())
                .first::<Checkpoint>(conn)
        })
        .context("Failed reading total transaction number")?;
        Ok(checkpoint.network_total_transactions)
    }

    fn get_transaction_by_digest(&self, tx_digest: &str) -> Result<Transaction, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            transactions::dsl::transactions
                .filter(transactions::dsl::transaction_digest.eq(tx_digest))
                .first::<Transaction>(conn)
        })
        .context(&format!(
            "Failed reading transaction with digest {tx_digest}"
        ))
    }

    fn compose_sui_transaction_block_response(
        &self,
        tx: Transaction,
        options: Option<&SuiTransactionBlockResponseOptions>,
    ) -> Result<SuiTransactionBlockResponse, IndexerError> {
        let sender_signed_data: SenderSignedData =
            bcs::from_bytes(&tx.raw_transaction).map_err(|err| {
                IndexerError::InsertableParsingError(format!(
                    "Failed converting transaction BCS to SenderSignedData with error: {:?}",
                    err
                ))
            })?;
        let transaction = SuiTransactionBlock::try_from(sender_signed_data, &self.module_cache)?;
        let effects: SuiTransactionBlockEffects = serde_json::from_str(&tx.transaction_effects_content).map_err(|err| {
        IndexerError::InsertableParsingError(format!(
            "Failed converting transaction effect JSON {:?} to SuiTransactionBlockEffects with error: {:?}",
            tx.transaction_effects_content, err
        ))
        })?;

        let tx_digest: TransactionDigest = tx.transaction_digest.parse().map_err(|e| {
            IndexerError::InsertableParsingError(format!(
                "Failed to parse transaction digest {} : {:?}",
                tx.transaction_digest, e
            ))
        })?;

        let (mut tx_opt, mut effects_opt, mut raw_tx) = (None, None, vec![]);
        let (object_changes, balance_changes, mut events) = (None, None, None);
        if let Some(options) = options {
            //TODO Object changes and balance changes are not supported atm due to async issues
            if options.show_balance_changes {
                return Err(IndexerError::NotSupportedError(
                    "Object cache is not supported".to_string(),
                ));
                // let object_cache = ObjectProviderCache::new(self.clone());
                // balance_changes =
                //     Some(get_balance_changes_from_effect(&object_cache, &effects).await?);
            }
            if options.show_object_changes {
                return Err(IndexerError::NotSupportedError(
                    "Object cache is not supported".to_string(),
                ));
                // let object_cache = ObjectProviderCache::new(self.clone());
                // object_changes = Some(
                //     get_object_changes(
                //         &object_cache,
                //         sender,
                //         &effects.modified_at_versions(),
                //         effects.all_changed_objects(),
                //         effects.all_deleted_objects(),
                //     )
                //     .await?,
                // );
            }
            if options.show_events {
                let event_page = self.get_events(
                    EventFilter::Transaction(tx_digest),
                    None,
                    None,
                    /* descending_order */ false,
                )?;
                events = Some(SuiTransactionBlockEvents {
                    data: event_page.data,
                });
            }
            if options.show_input {
                tx_opt = Some(transaction);
            }
            if options.show_raw_input {
                raw_tx = tx.raw_transaction;
            }
            if options.show_effects {
                effects_opt = Some(effects);
            }
        }

        Ok(SuiTransactionBlockResponse {
            digest: tx_digest,
            transaction: tx_opt,
            raw_transaction: raw_tx,
            effects: effects_opt,
            confirmed_local_execution: tx.confirmed_local_execution,
            timestamp_ms: tx.timestamp_ms.map(|t| t as u64),
            checkpoint: tx.checkpoint_sequence_number.map(|c| c as u64),
            events,
            object_changes,
            balance_changes,
            errors: vec![],
        })
    }

    fn multi_get_transactions_by_digests(
        &self,
        tx_digests: &[String],
    ) -> Result<Vec<Transaction>, IndexerError> {
        let transactions = read_only_blocking!(&self.blocking_cp, |conn| {
            transactions::dsl::transactions
                .filter(transactions::dsl::transaction_digest.eq_any(tx_digests))
                .load::<Transaction>(conn)
        })
        .context(&format!(
            "Failed reading transactions with digests {tx_digests:?}"
        ))?;
        // sort the transactions by the order of the input digests
        let digest_indices: BTreeMap<_, _> = tx_digests
            .iter()
            .enumerate()
            .map(|(i, digest)| (digest, i))
            .collect();
        let mut sorted_transactions: Vec<_> = transactions.into_iter().collect();
        sorted_transactions.sort_by_key(|tx| digest_indices.get(&tx.transaction_digest));
        Ok(sorted_transactions)
    }

    fn get_transaction_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            if let Some(digest) = &tx_digest {
                let mut boxed_query = transactions::dsl::transactions
                    .filter(transactions::dsl::transaction_digest.eq(digest))
                    .select(transactions::dsl::id)
                    .into_boxed();
                if is_descending {
                    boxed_query = boxed_query.order(transactions::dsl::id.desc());
                } else {
                    boxed_query = boxed_query.order(transactions::dsl::id.asc());
                }
                Some(boxed_query.first::<i64>(conn))
            } else {
                None
            }
            .transpose()
        })
        .context(&format!(
            "Failed reading transaction sequence with digest {tx_digest:?}"
        ))
    }

    fn get_object(
        &self,
        object_id: ObjectID,
        version: Option<SequenceNumber>,
    ) -> Result<ObjectRead, IndexerError> {
        // MUSTFIX (jian): add display field error support on implementation
        let object = read_only_blocking!(&self.blocking_cp, |conn| {
            if let Some(version) = version {
                objects_history::dsl::objects_history
                    .select((
                        objects_history::epoch,
                        objects_history::checkpoint,
                        objects_history::object_id,
                        objects_history::version,
                        objects_history::object_digest,
                        objects_history::owner_type,
                        objects_history::owner_address,
                        objects_history::initial_shared_version,
                        objects_history::previous_transaction,
                        objects_history::object_type,
                        objects_history::object_status,
                        objects_history::has_public_transfer,
                        objects_history::storage_rebate,
                        objects_history::bcs,
                    ))
                    .filter(objects_history::object_id.eq(object_id.to_string()))
                    .filter(objects_history::version.eq(version.value() as i64))
                    .get_result::<Object>(conn)
                    .optional()
            } else {
                objects::dsl::objects
                    .filter(objects::dsl::object_id.eq(object_id.to_string()))
                    .first::<Object>(conn)
                    .optional()
            }
        })
        .context(&format!("Failed reading object with id {object_id}"))?;

        match object {
            None => Ok(ObjectRead::NotExists(object_id)),
            Some(o) => o.try_into_object_read(&self.module_cache),
        }
    }

    fn query_objects_history(
        &self,
        filter: SuiObjectDataFilter,
        at_checkpoint: CheckpointSequenceNumber,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<ObjectRead>, IndexerError> {
        let objects = read_only_blocking!(&self.blocking_cp, |conn| {
            let columns = vec![
                "epoch",
                "checkpoint",
                "object_id",
                "version",
                "object_digest",
                "owner_type",
                "owner_address",
                "initial_shared_version",
                "previous_transaction",
                "object_type",
                "object_status",
                "has_public_transfer",
                "storage_rebate",
                "bcs",
            ];
            diesel::sql_query(filter.to_objects_history_sql(cursor, limit, columns))
                .bind::<BigInt, _>(at_checkpoint as i64)
                .get_results::<Object>(conn)
        })?;

        objects
            .into_iter()
            .map(|object| object.try_into_object_read(&self.module_cache))
            .collect()
    }

    // NOTE(gegaowp): now only supports query by address owner
    fn query_latest_objects(
        &self,
        filter: SuiObjectDataFilter,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<ObjectRead>, IndexerError> {
        let columns = vec![
            "epoch",
            "checkpoint",
            "object_id",
            "version",
            "object_digest",
            "owner_type",
            "owner_address",
            "initial_shared_version",
            "previous_transaction",
            "object_type",
            "object_status",
            "has_public_transfer",
            "storage_rebate",
            "bcs",
        ];

        let objects = read_only_blocking!(&self.blocking_cp, |conn| diesel::sql_query(
            filter.to_latest_objects_sql(cursor, limit, columns)
        )
        .get_results::<Object>(conn))?;

        objects
            .into_iter()
            .map(|object| object.try_into_object_read(&self.module_cache))
            .collect()
    }

    fn get_move_call_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            if let Some(digest) = &tx_digest {
                let mut boxed_query = move_calls::dsl::move_calls
                    .filter(move_calls::dsl::transaction_digest.eq(digest))
                    .select(move_calls::dsl::id)
                    .into_boxed();
                if is_descending {
                    boxed_query = boxed_query.order(move_calls::dsl::id.desc());
                } else {
                    boxed_query = boxed_query.order(move_calls::dsl::id.asc());
                }
                Some(boxed_query.first::<i64>(conn))
            } else {
                None
            }
            .transpose()
        })
        .context(&format!(
            "Failed reading move call sequence with digest {tx_digest:?}"
        ))
    }

    fn get_input_object_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            if let Some(digest) = &tx_digest {
                let mut boxed_query = input_objects::dsl::input_objects
                    .filter(input_objects::dsl::transaction_digest.eq(digest))
                    .select(input_objects::dsl::id)
                    .into_boxed();
                if is_descending {
                    boxed_query = boxed_query.order(input_objects::dsl::id.desc());
                } else {
                    boxed_query = boxed_query.order(input_objects::dsl::id.asc());
                }
                Some(boxed_query.first::<i64>(conn))
            } else {
                None
            }
            .transpose()
        })
        .context(&format!(
            "Failed reading input object sequence with digest {tx_digest:?}"
        ))
    }

    fn get_changed_object_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            if let Some(digest) = &tx_digest {
                let mut boxed_query = changed_objects::dsl::changed_objects
                    .filter(changed_objects::dsl::transaction_digest.eq(digest))
                    .select(changed_objects::dsl::id)
                    .into_boxed();
                if is_descending {
                    boxed_query = boxed_query.order(changed_objects::dsl::id.desc());
                } else {
                    boxed_query = boxed_query.order(changed_objects::dsl::id.asc());
                }
                Some(boxed_query.first::<i64>(conn))
            } else {
                None
            }
            .transpose()
        })
        .context(&format!(
            "Failed reading changed object sequence with digest {tx_digest:?}"
        ))
    }

    fn get_recipient_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            if let Some(digest) = &tx_digest {
                let mut boxed_query = recipients::dsl::recipients
                    .filter(recipients::dsl::transaction_digest.eq(digest))
                    .select(recipients::dsl::id)
                    .into_boxed();
                // NOTE: if the query is ORDER BY id ASC, we want to skip the LAST
                // recipient of the cursor transaction, thus we should order by id DESC;
                // Same thing for the other way around.
                if is_descending {
                    boxed_query = boxed_query.order(recipients::dsl::id.asc());
                } else {
                    boxed_query = boxed_query.order(recipients::dsl::id.desc());
                }
                Some(boxed_query.first::<i64>(conn))
            } else {
                None
            }
            .transpose()
        })
        .context(&format!(
            "Failed reading recipients sequence with digest {tx_digest:?}"
        ))
    }

    fn get_all_transaction_page(
        &self,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let mut boxed_query = transactions::dsl::transactions.into_boxed();
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    boxed_query = boxed_query.filter(transactions::dsl::id.lt(start_sequence));
                } else {
                    boxed_query = boxed_query.filter(transactions::dsl::id.gt(start_sequence));
                }
            }

            if is_descending {
                boxed_query
                    .order(transactions::dsl::id.desc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            } else {
                boxed_query
                    .order(transactions::dsl::id.asc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            }
        }).context(&format!("Failed reading all transaction digests with start_sequence {start_sequence:?} and limit {limit}"))
    }

    fn get_transaction_page_by_checkpoint(
        &self,
        checkpoint_sequence_number: i64,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let mut boxed_query = transactions::dsl::transactions
                .filter(transactions::dsl::checkpoint_sequence_number.eq(checkpoint_sequence_number))
                .into_boxed();
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    boxed_query = boxed_query.filter(transactions::dsl::id.lt(start_sequence));
                } else {
                    boxed_query = boxed_query.filter(transactions::dsl::id.gt(start_sequence));
                }
            }
            if is_descending {
                boxed_query
                    .order(transactions::dsl::id.desc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            } else {
                boxed_query
                    .order(transactions::dsl::id.asc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            }
        }).context(&format!("Failed reading transaction digests with checkpoint_sequence_number {checkpoint_sequence_number:?} and start_sequence {start_sequence:?} and limit {limit}"))
    }

    fn get_transaction_page_by_transaction_kinds(
        &self,
        kinds: Vec<String>,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let mut boxed_query = transactions::dsl::transactions
                .filter(transactions::dsl::transaction_kind.eq_any(kinds.clone()))
                .into_boxed();
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    boxed_query = boxed_query.filter(transactions::dsl::id.lt(start_sequence));
                } else {
                    boxed_query = boxed_query.filter(transactions::dsl::id.gt(start_sequence));
                }
            }

            if is_descending {
                boxed_query
                    .order(transactions::dsl::id.desc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            } else {
               boxed_query
                    .order(transactions::dsl::id.asc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            }
        }).context(&format!("Failed reading transaction digests with kind {kinds:?} and start_sequence {start_sequence:?} and limit {limit}"))
    }

    fn get_transaction_page_by_sender_address(
        &self,
        sender_address: String,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let mut boxed_query = transactions::dsl::transactions
                .filter(transactions::dsl::sender.eq(sender_address.clone()))
                .into_boxed();
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    boxed_query = boxed_query.filter(transactions::dsl::id.lt(start_sequence));
                } else {
                    boxed_query = boxed_query.filter(transactions::dsl::id.gt(start_sequence));
                }
            }

            if is_descending {
                boxed_query
                    .order(transactions::dsl::id.desc())
                    .limit(limit as i64)
                    .load::<Transaction>(conn)
            } else {
               boxed_query
                    .order(transactions::dsl::id.asc())
                    .limit(limit as i64)
                    .load::<Transaction>(conn)
            }
        }).context(&format!("Failed reading transaction digests by sender address {sender_address} with start_sequence {start_sequence:?} and limit {limit}"))
    }

    fn get_transaction_page_by_input_object(
        &self,
        object_id: ObjectID,
        version: Option<i64>,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        let sql_query = format!(
            "SELECT transaction_digest as digest_name
             FROM input_objects
             WHERE object_id = '{}' {} {}
             ORDER BY id {} LIMIT {}",
            object_id,
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    format!("AND id < {}", start_sequence)
                } else {
                    format!("AND id > {}", start_sequence)
                }
            } else {
                "".to_string()
            },
            if let Some(version) = version {
                format!("AND version = {}", version)
            } else {
                "".to_string()
            },
            if is_descending { "DESC" } else { "ASC" },
            limit
        );
        let tx_digests: Vec<String> = read_only_blocking!(&self.blocking_cp, |conn| diesel::sql_query(sql_query).load(conn))
                .context(&format!("Failed reading transaction digests by input object ID {object_id} and version {version:?} with start_sequence {start_sequence:?} and limit {limit}"))?
                .into_iter()
                .map(|table: TempDigestTable| table.digest_name)
                .collect();
        self.multi_get_transactions_by_digests(&tx_digests)
    }

    fn get_transaction_page_by_changed_object(
        &self,
        object_id: ObjectID,
        version: Option<i64>,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        let sql_query = format!(
            "SELECT transaction_digest as digest_name
             FROM changed_objects
             WHERE object_id = '{}' {} {}
             ORDER BY id {} LIMIT {}",
            object_id,
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    format!("AND id < {}", start_sequence)
                } else {
                    format!("AND id > {}", start_sequence)
                }
            } else {
                "".to_string()
            },
            if let Some(version) = version {
                format!("AND version = {}", version)
            } else {
                "".to_string()
            },
            if is_descending { "DESC" } else { "ASC" },
            limit
        );
        let tx_digests: Vec<String> = read_only_blocking!(&self.blocking_cp, |conn| diesel::sql_query(sql_query).load(conn))
                .context(&format!("Failed reading transaction digests by changed object ID {object_id} and version {version:?} with start_sequence {start_sequence:?} and limit {limit}"))?
                .into_iter()
                .map(|table: TempDigestTable| table.digest_name)
                .collect();
        self.multi_get_transactions_by_digests(&tx_digests)
    }

    fn get_transaction_page_by_move_call(
        &self,
        package_name: ObjectID,
        module_name: Option<Identifier>,
        function_name: Option<Identifier>,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        // note: module_name and function_name are user-controlled, which is scary.
        // however, but valid Move identifiers can only contain 0-9, a-z, A-Z, and _,
        // so it is safe to use them as-is in the query below
        let sql_query = format!(
            "SELECT transaction_digest as digest_name
             FROM move_calls
             WHERE move_package = '{}' {} {} {}
             ORDER BY id {} LIMIT {}",
            package_name,
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    format!("AND id < {}", start_sequence)
                } else {
                    format!("AND id > {}", start_sequence)
                }
            } else {
                "".to_string()
            },
            if let Some(module_name) = module_name.clone() {
                format!("AND move_module = '{}'", module_name)
            } else {
                "".to_string()
            },
            if let Some(function_name) = function_name.clone() {
                format!("AND move_function = '{}'", function_name)
            } else {
                "".to_string()
            },
            if is_descending { "DESC" } else { "ASC" },
            limit
        );
        let tx_digests: Vec<String> = read_only_blocking!(&self.blocking_cp, |conn| diesel::sql_query(sql_query).load(conn))
                .context(&format!(
                        "Failed reading transaction digests with package_name {} module_name {:?} and function_name {:?} and start_sequence {:?} and limit {}",
                        package_name, module_name, function_name, start_sequence, limit))?
                .into_iter()
                .map(|table: TempDigestTable| table.digest_name)
                .collect();
        self.multi_get_transactions_by_digests(&tx_digests)
    }

    fn get_transaction_page_by_recipient_address(
        &self,
        from: Option<SuiAddress>,
        to: SuiAddress,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        let sql_query = format!(
            "SELECT transaction_digest as digest_name FROM recipients
             WHERE recipient = '{}' {} {}
             ORDER BY id {} LIMIT {}",
            to,
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    format!("AND id < {}", start_sequence)
                } else {
                    format!("AND id > {}", start_sequence)
                }
            } else {
                "".to_string()
            },
            if let Some(from) = from {
                format!("AND sender = '{}'", from)
            } else {
                "".to_string()
            },
            if is_descending { "DESC" } else { "ASC" },
            limit
        );
        let tx_digests: Vec<String> = read_only_blocking!(&self.blocking_cp, |conn| diesel::sql_query(sql_query).load(conn))
                .context(&format!("Failed reading transaction digests by recipient address {to} with start_sequence {start_sequence:?} and limit {limit}"))?
                .into_iter()
                .map(|table: TempDigestTable| table.digest_name)
                .collect();
        self.multi_get_transactions_by_digests(&tx_digests)
    }

    fn get_transaction_page_by_address(
        &self,
        address: SuiAddress,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        let sql_query = format!(
            "SELECT transaction_digest as digest_name FROM (
                SELECT transaction_digest, max(id) AS max_id
                FROM recipients
                WHERE recipient = '{}' OR sender = '{}'
                {} GROUP BY transaction_digest
                ORDER BY max_id {} LIMIT {}
            ) AS t",
            address,
            address,
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    format!("AND id < {}", start_sequence)
                } else {
                    format!("AND id > {}", start_sequence)
                }
            } else {
                "".to_string()
            },
            if is_descending { "DESC" } else { "ASC" },
            limit
        );
        let tx_digests: Vec<String> = read_only_blocking!(&self.blocking_cp, |conn| diesel::sql_query(sql_query).load(conn))
                .context(&format!("Failed reading transaction digests by address {address} with start_sequence {start_sequence:?} and limit {limit}"))?
                .into_iter()
                .map(|table: TempDigestTable| table.digest_name)
                .collect();
        self.multi_get_transactions_by_digests(&tx_digests)
    }

    fn get_network_metrics(&self) -> Result<NetworkMetrics, IndexerError> {
        get_network_metrics_cached(&self.blocking_cp)
    }

    fn get_move_call_metrics(&self) -> Result<MoveCallMetrics, IndexerError> {
        let metrics = read_only_blocking!(&self.blocking_cp, |conn| {
            diesel::sql_query("SELECT * FROM epoch_move_call_metrics;")
                .get_results::<DBMoveCallMetrics>(conn)
        })?;

        let mut d3 = vec![];
        let mut d7 = vec![];
        let mut d30 = vec![];
        for m in metrics {
            let package = ObjectID::from_str(&m.move_package);
            let module = Identifier::from_str(m.move_module.as_str());
            let function = Identifier::from_str(m.move_function.as_str());
            if let (Ok(package), Ok(module), Ok(function)) = (package, module, function) {
                let fun = MoveFunctionName {
                    package,
                    module,
                    function,
                };
                match m.day {
                    3 => d3.push((fun, m.count as usize)),
                    7 => d7.push((fun, m.count as usize)),
                    30 => d30.push((fun, m.count as usize)),
                    _ => {}
                }
            }
        }
        Ok(MoveCallMetrics {
            rank_3_days: d3,
            rank_7_days: d7,
            rank_30_days: d30,
        })
    }

    fn persist_fast_path(
        &self,
        tx: Transaction,
        tx_object_changes: TransactionObjectChanges,
    ) -> Result<usize, IndexerError> {
        transactional_blocking!(&self.blocking_cp, |conn| {
            diesel::insert_into(transactions::table)
                .values(vec![tx])
                .on_conflict_do_nothing()
                .execute(conn)
                .map_err(IndexerError::from)
                .context("Failed writing transactions to PostgresDB")?;

            let deleted_objects: Vec<Object> = tx_object_changes
                .deleted_objects
                .iter()
                .map(|deleted_object| deleted_object.clone().into())
                .collect();

            persist_transaction_object_changes(
                conn,
                tx_object_changes.changed_objects,
                deleted_objects,
                None,
                None,
                None,
            )
        })
    }

    fn persist_checkpoint_transactions(
        &self,
        checkpoint: &Checkpoint,
        transactions: &[Transaction],
        total_transaction_chunk_committed_counter: IntCounter,
    ) -> Result<usize, IndexerError> {
        transactional_blocking!(&self.blocking_cp, |conn| {
            // Commit indexed transactions
            for transaction_chunk in transactions.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(transactions::table)
                    .values(transaction_chunk)
                    .on_conflict(transactions::transaction_digest)
                    .do_update()
                    .set((
                        transactions::timestamp_ms.eq(excluded(transactions::timestamp_ms)),
                        transactions::checkpoint_sequence_number
                            .eq(excluded(transactions::checkpoint_sequence_number)),
                    ))
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing transactions to PostgresDB")?;
            }
            // chunks.counter() will consume the iterator, so we need to calculate the number of chunks separately.
            let num_chunks = (transactions.len() + PG_COMMIT_CHUNK_SIZE - 1) / PG_COMMIT_CHUNK_SIZE;
            total_transaction_chunk_committed_counter.inc_by(num_chunks as u64);

            // Commit indexed checkpoint last, so that if the checkpoint is committed,
            // all related data have been committed as well.
            diesel::insert_into(checkpoints::table)
                .values(checkpoint)
                .on_conflict_do_nothing()
                .execute(conn)
                .map_err(IndexerError::from)
                .context("Failed writing checkpoint to PostgresDB")
        })
    }

    fn persist_object_changes(
        &self,
        tx_object_changes: &[TransactionObjectChanges],
        total_object_change_chunk_committed_counter: IntCounter,
        object_mutation_latency: Histogram,
        object_deletion_latency: Histogram,
    ) -> Result<(), IndexerError> {
        transactional_blocking!(&self.blocking_cp, |conn| {
            let mutated_objects: Vec<Object> = tx_object_changes
                .iter()
                .flat_map(|changes| changes.changed_objects.iter().cloned())
                .collect();
            let deleted_changes = tx_object_changes
                .iter()
                .flat_map(|changes| changes.deleted_objects.iter().cloned())
                .collect::<Vec<_>>();
            let deleted_objects: Vec<Object> = deleted_changes
                .iter()
                .map(|deleted_object| deleted_object.clone().into())
                .collect();
            persist_transaction_object_changes(
                conn,
                mutated_objects,
                deleted_objects,
                Some(total_object_change_chunk_committed_counter),
                Some(object_mutation_latency),
                Some(object_deletion_latency),
            )?;
            Ok::<(), IndexerError>(())
        })?;
        Ok(())
    }

    fn persist_events(&self, events: &[Event]) -> Result<(), IndexerError> {
        transactional_blocking!(&self.blocking_cp, |conn| {
            for event_chunk in events.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(events::table)
                    .values(event_chunk)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing events to PostgresDB")?;
            }
            Ok::<(), IndexerError>(())
        })?;
        Ok(())
    }

    fn persist_addresses(
        &self,
        addresses: &[Address],
        active_addresses: &[ActiveAddress],
    ) -> Result<(), IndexerError> {
        transactional_blocking!(&self.blocking_cp, |conn| {
            for address_chunk in addresses.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(addresses::table)
                    .values(address_chunk)
                    .on_conflict(addresses::account_address)
                    .do_update()
                    .set((
                        addresses::last_appearance_time
                            .eq(excluded(addresses::last_appearance_time)),
                        addresses::last_appearance_tx.eq(excluded(addresses::last_appearance_tx)),
                    ))
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context(
                        format!(
                            "Failed writing addresses to PostgresDB with tx {}",
                            address_chunk[0].last_appearance_tx
                        )
                        .as_str(),
                    )?;
            }
            for active_address_chunk in active_addresses.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(active_addresses::table)
                    .values(active_address_chunk)
                    .on_conflict(active_addresses::account_address)
                    .do_update()
                    .set((
                        active_addresses::last_appearance_time
                            .eq(excluded(active_addresses::last_appearance_time)),
                        active_addresses::last_appearance_tx
                            .eq(excluded(active_addresses::last_appearance_tx)),
                    ))
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context(
                        format!(
                            "Failed writing active addresses to PostgresDB with tx {}",
                            active_address_chunk[0].last_appearance_tx
                        )
                        .as_str(),
                    )?;
            }
            Ok::<(), IndexerError>(())
        })?;
        Ok(())
    }
    fn persist_packages(&self, packages: &[Package]) -> Result<(), IndexerError> {
        transactional_blocking!(&self.blocking_cp, |conn| {
            for packages_chunk in packages.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(packages::table)
                    .values(packages_chunk)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing packages to PostgresDB")?;
            }
            Ok::<(), IndexerError>(())
        })?;
        Ok(())
    }

    fn persist_transaction_index_tables(
        &self,
        input_objects: &[InputObject],
        changed_objects: &[ChangedObject],
        move_calls: &[MoveCall],
        recipients: &[Recipient],
    ) -> Result<(), IndexerError> {
        transactional_blocking!(&self.blocking_cp, |conn| {
            // Commit indexed move calls
            for move_calls_chunk in move_calls.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(move_calls::table)
                    .values(move_calls_chunk)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing move_calls to PostgresDB")?;
            }

            // Commit indexed input objects
            for input_objects_chunk in input_objects.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(input_objects::table)
                    .values(input_objects_chunk)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing input_objects to PostgresDB")?;
            }

            // Commit indexed changed objects
            for changed_objects_chunk in changed_objects.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(changed_objects::table)
                    .values(changed_objects_chunk)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing changed_objects to PostgresDB")?;
            }

            // Commit indexed recipients
            for recipients_chunk in recipients.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(recipients::table)
                    .values(recipients_chunk)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing recipients to PostgresDB")?;
            }
            Ok::<(), IndexerError>(())
        })?;
        Ok(())
    }

    fn get_network_total_transactions_previous_epoch(
        &self,
        epoch: i64,
    ) -> Result<i64, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints::table
                .filter(checkpoints::epoch.eq(epoch - 1))
                .select(max(checkpoints::network_total_transactions))
                .first::<Option<i64>>(conn)
                .map(|o| o.unwrap_or(0))
        })
        .context("Failed to count network transactions in previous epoch")
    }

    fn persist_epoch(&self, data: &TemporaryEpochStore) -> Result<(), IndexerError> {
        // MUSTFIX(gegaowp): temporarily disable the epoch advance logic.
        // let last_epoch_cp_id = if data.last_epoch.is_none() {
        //     0
        // } else {
        //     self.get_current_epoch()?.first_checkpoint_id as i64
        // };
        if data.last_epoch.is_none() {
            let last_epoch_cp_id = 0;
            self.partition_manager
                .advance_epoch(&data.new_epoch, last_epoch_cp_id)?;
        }
        transactional_blocking!(&self.blocking_cp, |conn| {
            if let Some(last_epoch) = &data.last_epoch {
                info!("Persisting at the end of epoch {}", last_epoch.epoch);
                diesel::insert_into(epochs::table)
                    .values(last_epoch)
                    .on_conflict(epochs::epoch)
                    .do_update()
                    .set((
                        epochs::last_checkpoint_id.eq(excluded(epochs::last_checkpoint_id)),
                        epochs::epoch_end_timestamp.eq(excluded(epochs::epoch_end_timestamp)),
                        epochs::epoch_total_transactions
                            .eq(excluded(epochs::epoch_total_transactions)),
                        epochs::protocol_version.eq(excluded(epochs::protocol_version)),
                        epochs::next_epoch_version.eq(excluded(epochs::next_epoch_version)),
                        epochs::next_epoch_committee.eq(excluded(epochs::next_epoch_committee)),
                        epochs::next_epoch_committee_stake
                            .eq(excluded(epochs::next_epoch_committee_stake)),
                        epochs::epoch_commitments.eq(excluded(epochs::epoch_commitments)),
                        epochs::reference_gas_price.eq(excluded(epochs::reference_gas_price)),
                        epochs::total_stake.eq(excluded(epochs::total_stake)),
                        epochs::storage_fund_reinvestment
                            .eq(excluded(epochs::storage_fund_reinvestment)),
                        epochs::storage_charge.eq(excluded(epochs::storage_charge)),
                        epochs::storage_rebate.eq(excluded(epochs::storage_rebate)),
                        epochs::storage_fund_balance.eq(excluded(epochs::storage_fund_balance)),
                        epochs::stake_subsidy_amount.eq(excluded(epochs::stake_subsidy_amount)),
                        epochs::total_gas_fees.eq(excluded(epochs::total_gas_fees)),
                        epochs::total_stake_rewards_distributed
                            .eq(excluded(epochs::total_stake_rewards_distributed)),
                        epochs::leftover_storage_fund_inflow
                            .eq(excluded(epochs::leftover_storage_fund_inflow)),
                    ))
                    .execute(conn)?;
                info!("Persisted epoch {}", last_epoch.epoch);
            }
            diesel::insert_into(system_states::table)
                .values(&data.system_state)
                .on_conflict_do_nothing()
                .execute(conn)?;

            diesel::insert_into(validators::table)
                .values(&data.validators)
                .on_conflict_do_nothing()
                .execute(conn)
        })?;
        info!("Persisting initial state of epoch {}", data.new_epoch.epoch);
        transactional_blocking!(&self.blocking_cp, |conn| {
            diesel::insert_into(epochs::table)
                .values(&data.new_epoch)
                .on_conflict_do_nothing()
                .execute(conn)
        })?;
        info!("Persisted initial state of epoch {}", data.new_epoch.epoch);
        Ok(())
    }

    fn get_epochs(
        &self,
        cursor: Option<EpochId>,
        limit: usize,
        descending_order: Option<bool>,
    ) -> Result<Vec<EpochInfo>, IndexerError> {
        let is_descending = descending_order.unwrap_or_default();
        let id = cursor
            .map(|id| id as i64)
            .unwrap_or(if is_descending { i64::MAX } else { -1 });
        let mut query = epochs::dsl::epochs.into_boxed();
        if is_descending {
            query = query
                .filter(epochs::epoch.lt(id))
                .order_by(epochs::epoch.desc());
        } else {
            query = query
                .filter(epochs::epoch.gt(id))
                .order_by(epochs::epoch.asc());
        }

        let epoch_info: Vec<DBEpochInfo> = read_only_blocking!(&self.blocking_cp, |conn| query
            .limit(limit as i64)
            .load(conn))
        .map_err(|e| {
            IndexerError::PostgresReadError(format!(
                "Failed reading epochs from PostgresDB with error {:?}",
                e
            ))
        })?;

        let validators: Vec<DBValidatorSummary> =
            read_only_blocking!(&self.blocking_cp, |conn| validators::dsl::validators
                .filter(validators::epoch.gt(id))
                .load(conn))
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Failed reading validators from PostgresDB with error {:?}",
                    e
                ))
            })?;

        let mut validators =
            validators
                .into_iter()
                .fold(BTreeMap::<i64, Vec<_>>::new(), |mut acc, v| {
                    acc.entry(v.epoch).or_default().push(v);
                    acc
                });

        epoch_info
            .into_iter()
            .map(|info| {
                let epoch = info.epoch;
                info.to_epoch_info(validators.remove(&epoch).unwrap_or_default())
            })
            .collect()
    }

    fn get_current_epoch(&self) -> Result<EpochInfo, IndexerError> {
        let epoch_info: DBEpochInfo = read_only_blocking!(&self.blocking_cp, |conn| {
            epochs::dsl::epochs
                .order_by(epochs::epoch.desc())
                .first::<DBEpochInfo>(conn)
        })
        .context("Failed reading current epoch")?;

        let validators: Vec<DBValidatorSummary> = read_only_blocking!(&self.blocking_cp, |conn| {
            validators::dsl::validators
                .filter(validators::epoch.eq(epoch_info.epoch))
                .load(conn)
        })
        .context("Failed reading latest validator summary")?;

        epoch_info.to_epoch_info(validators)
    }

    /// address stats methods
    fn get_last_address_processed_checkpoint(&self) -> Result<i64, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            address_stats::dsl::address_stats
                .select(max(address_stats::checkpoint))
                .first::<Option<i64>>(conn)
                // -1 to differentiate between no checkpoints and the first checkpoint
                .map(|o| o.unwrap_or(-1))
        })
        .context("Failed reading latest object checkpoint from PostgresDB")
    }

    fn calculate_address_stats(&self, checkpoint: i64) -> Result<AddressStats, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let cp: Checkpoint = checkpoints::dsl::checkpoints
                .filter(checkpoints::sequence_number.eq(checkpoint))
                .limit(1)
                .first(conn)?;
            let cp_timestamp_ms = cp.timestamp_ms;
            let cumulative_addresses = addresses::dsl::addresses
                .filter(addresses::first_appearance_time.le(cp_timestamp_ms))
                .select(count(addresses::account_address))
                .first(conn)?;
            let cumulative_active_addresses = active_addresses::dsl::active_addresses
                .filter(active_addresses::first_appearance_time.le(cp_timestamp_ms))
                .select(count(active_addresses::account_address))
                .first(conn)?;
            let time_one_day_ago = cp_timestamp_ms - 1000 * 60 * 60 * 24;
            let daily_active_addresses = active_addresses::dsl::active_addresses
                .filter(active_addresses::first_appearance_time.le(cp_timestamp_ms))
                .filter(active_addresses::last_appearance_time.gt(time_one_day_ago))
                .select(count(active_addresses::account_address))
                .first(conn)?;
            Ok::<AddressStats, IndexerError>(AddressStats {
                checkpoint: cp.sequence_number,
                epoch: cp.epoch,
                timestamp_ms: cp_timestamp_ms,
                cumulative_addresses,
                cumulative_active_addresses,
                daily_active_addresses,
            })
        })
        .context("Failed reading latest object checkpoint sequence number from PostgresDB")
    }

    fn persist_address_stats(&self, addr_stats: &AddressStats) -> Result<(), IndexerError> {
        transactional_blocking!(&self.blocking_cp, |conn| {
            diesel::insert_into(address_stats::dsl::address_stats)
                .values(addr_stats)
                .execute(conn)
        })
        .context("Failed persisting address stats to PostgresDB")?;
        Ok(())
    }

    fn get_latest_address_stats(&self) -> Result<AddressStats, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            address_stats::dsl::address_stats
                .order_by(address_stats::checkpoint.desc())
                .first(conn)
        })
        .context("Failed reading latest address stats from PostgresDB")
    }

    fn get_checkpoint_address_stats(&self, checkpoint: i64) -> Result<AddressStats, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            address_stats::dsl::address_stats
                .filter(address_stats::checkpoint.eq(checkpoint))
                .order_by(address_stats::epoch.asc())
                .first(conn)
        })
        .context(
            format!(
                "Failed reading address stats of checkpoint {} from PostgresDB",
                checkpoint
            )
            .as_str(),
        )
    }

    fn get_all_epoch_address_stats(
        &self,
        descending_order: Option<bool>,
    ) -> Result<Vec<AddressStats>, IndexerError> {
        let is_descending = descending_order.unwrap_or_default();
        let epoch_addr_stats_query = format!(
            "WITH ranked_rows AS (
                SELECT
                  checkpoint, epoch, timestamp_ms, cumulative_addresses, cumulative_active_addresses, daily_active_addresses,
                  row_number() OVER(PARTITION BY epoch ORDER BY checkpoint DESC) as row_num
                FROM
                  address_stats
              )
              SELECT
                checkpoint, epoch, timestamp_ms, cumulative_addresses, cumulative_active_addresses, daily_active_addresses
              FROM ranked_rows
              WHERE row_num = 1 ORDER BY epoch {}",
              if is_descending { "DESC" } else { "ASC" },
        );

        let db_addr_stats = read_only_blocking!(&self.blocking_cp, |conn| diesel::sql_query(
            epoch_addr_stats_query
        )
        .load::<DBAddressStats>(conn))
        .context("Failed reading all epoch address stats from PostgresDB")?;
        Ok(db_addr_stats
            .into_iter()
            .map(|db_addr_stats| db_addr_stats.into())
            .collect())
    }

    /// checkpoint metrics methods
    fn get_latest_checkpoint_metrics(&self) -> Result<CheckpointMetrics, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoint_metrics::dsl::checkpoint_metrics
                .order_by(checkpoint_metrics::checkpoint.desc())
                .first(conn)
        })
        .context("Failed reading latest checkpoint metrics from PostgresDB")
    }
    fn calculate_checkpoint_metrics(
        &self,
        current_checkpoint: i64,
        last_checkpoint_metrics: &CheckpointMetrics,
        cps: &[Checkpoint],
    ) -> Result<CheckpointMetrics, IndexerError> {
        let last_cp = cps.last().ok_or(IndexerError::PostgresReadError(
            "Failed to get last checkpoint while calculating checkpoint metrics".to_string(),
        ))?;
        let last_cp_timestamp_ms = last_cp.timestamp_ms;
        let peak_tps_30d = self.calculate_peak_tps_30d(current_checkpoint, last_cp_timestamp_ms)?;
        let real_time_tps = self.calculate_real_time_tps(current_checkpoint)?;

        let (
            rolling_tx_delta,
            rolling_tx_blocks_delta,
            rolling_successful_tx_delta,
            rolling_successful_tx_blocks_delta,
        ) = cps.iter().fold(
            (0, 0, 0, 0),
            |(tx_delta, tx_blocks_delta, successful_tx_delta, successful_tx_blocks_delta), cp| {
                (
                    tx_delta + cp.total_transactions,
                    tx_blocks_delta + cp.total_transaction_blocks,
                    successful_tx_delta + cp.total_successful_transactions,
                    successful_tx_blocks_delta + cp.total_successful_transaction_blocks,
                )
            },
        );

        Ok(CheckpointMetrics {
            checkpoint: current_checkpoint,
            epoch: last_cp.epoch,
            real_time_tps,
            peak_tps_30d,
            rolling_total_transactions: last_checkpoint_metrics.rolling_total_transactions
                + rolling_tx_delta,
            rolling_total_transaction_blocks: last_checkpoint_metrics
                .rolling_total_transaction_blocks
                + rolling_tx_blocks_delta,
            rolling_total_successful_transactions: last_checkpoint_metrics
                .rolling_total_successful_transactions
                + rolling_successful_tx_delta,
            rolling_total_successful_transaction_blocks: last_checkpoint_metrics
                .rolling_total_successful_transaction_blocks
                + rolling_successful_tx_blocks_delta,
        })
    }

    fn persist_checkpoint_metrics(
        &self,
        checkpoint_metrics: &CheckpointMetrics,
    ) -> Result<(), IndexerError> {
        transactional_blocking!(&self.blocking_cp, |conn| {
            diesel::insert_into(checkpoint_metrics::dsl::checkpoint_metrics)
                .values(checkpoint_metrics)
                .execute(conn)
        })
        .context("Failed persisting checkpoint metrics to PostgresDB")?;
        Ok(())
    }

    /// TPS related methods
    fn calculate_real_time_tps(&self, current_checkpoint: i64) -> Result<f64, IndexerError> {
        let real_time_tps_query = format!(
            "WITH recent_checkpoints AS (
                SELECT
                  sequence_number,
                  total_successful_transactions,
                  timestamp_ms
                FROM
                  checkpoints
                WHERE
                  sequence_number <= {}
                ORDER BY
                  timestamp_ms DESC
                LIMIT 100
              ),
              diff_checkpoints AS (
                SELECT
                  MAX(sequence_number) as sequence_number,
                  SUM(total_successful_transactions) as total_successful_transactions,
                  LAG(timestamp_ms) OVER (ORDER BY timestamp_ms DESC) - timestamp_ms AS time_diff
                FROM
                  recent_checkpoints
                GROUP BY
                  timestamp_ms
              )
              SELECT
                (total_successful_transactions * 1000.0 / time_diff)::float8 as tps
              FROM
                diff_checkpoints
              WHERE
                time_diff IS NOT NULL
              ORDER BY sequence_number DESC LIMIT 1;
            ",
            current_checkpoint
        );

        let real_time_tps: Tps =
            read_only_blocking!(&self.blocking_cp, |conn| diesel::RunQueryDsl::get_result(
                diesel::sql_query(real_time_tps_query),
                conn
            ))
            .context("Failed calculating peak TPS in 30 days from PostgresDB")?;
        Ok(real_time_tps.tps)
    }

    fn calculate_peak_tps_30d(
        &self,
        current_checkpoint: i64,
        current_timestamp_ms: i64,
    ) -> Result<f64, IndexerError> {
        let peak_tps_30d_query = format!(
            "WITH checkpoints_30d AS (
                SELECT
                  MAX(sequence_number) AS sequence_number,
                  SUM(total_successful_transactions) AS total_successful_transactions,
                  timestamp_ms
                FROM
                  checkpoints
                WHERE
                  sequence_number <= {} AND
                  timestamp_ms > ({} - 30::bigint * 24 * 60 * 60 * 1000)
                GROUP BY
                  timestamp_ms
              ),
              tps_data AS (
                SELECT
                  sequence_number,
                  total_successful_transactions,
                  timestamp_ms - LAG(timestamp_ms) OVER (ORDER BY sequence_number) AS time_diff
                FROM
                  checkpoints_30d
              )
              SELECT
                MAX(total_successful_transactions * 1000.0 / time_diff)::float8 as tps
              FROM
                tps_data
              WHERE
                time_diff IS NOT NULL;
            ",
            current_checkpoint, current_timestamp_ms
        );
        let peak_tps_30d: Tps =
            read_only_blocking!(&self.blocking_cp, |conn| diesel::RunQueryDsl::get_result(
                diesel::sql_query(peak_tps_30d_query),
                conn
            ))
            .context("Failed calculating peak TPS in 30 days from PostgresDB")?;
        Ok(peak_tps_30d.tps)
    }

    async fn spawn_blocking<F, R>(&self, f: F) -> Result<R, IndexerError>
    where
        F: FnOnce(Self) -> Result<R, IndexerError> + Send + 'static,
        R: Send + 'static,
    {
        let this = self.clone();
        tokio::task::spawn_blocking(move || f(this))
            .await
            .map_err(Into::into)
            .and_then(std::convert::identity)
    }
}

#[async_trait]
impl IndexerStore for PgIndexerStore {
    type ModuleCache = SyncModuleCache<IndexerModuleResolver>;

    async fn get_latest_tx_checkpoint_sequence_number(&self) -> Result<i64, IndexerError> {
        self.spawn_blocking(|this| this.get_latest_tx_checkpoint_sequence_number())
            .await
    }

    async fn get_latest_object_checkpoint_sequence_number(&self) -> Result<i64, IndexerError> {
        self.spawn_blocking(|this| this.get_latest_object_checkpoint_sequence_number())
            .await
    }

    async fn get_checkpoint(
        &self,
        id: CheckpointId,
    ) -> Result<sui_json_rpc_types::Checkpoint, IndexerError> {
        self.spawn_blocking(move |this| this.get_checkpoint(id))
            .await
    }

    async fn get_checkpoints(
        &self,
        cursor: Option<CheckpointId>,
        limit: usize,
    ) -> Result<Vec<sui_json_rpc_types::Checkpoint>, IndexerError> {
        self.spawn_blocking(move |this| this.get_checkpoints(cursor, limit))
            .await
    }

    async fn get_indexer_checkpoint(&self) -> Result<Checkpoint, IndexerError> {
        self.spawn_blocking(|this| this.get_indexer_checkpoint())
            .await
    }

    async fn get_indexer_checkpoints(
        &self,
        cursor: i64,
        limit: usize,
    ) -> Result<Vec<Checkpoint>, IndexerError> {
        self.spawn_blocking(move |this| this.get_indexer_checkpoints(cursor, limit))
            .await
    }

    async fn get_checkpoint_sequence_number(
        &self,
        digest: CheckpointDigest,
    ) -> Result<CheckpointSequenceNumber, IndexerError> {
        self.spawn_blocking(move |this| this.get_checkpoint_sequence_number(digest))
            .await
    }

    async fn get_event(&self, id: EventID) -> Result<Event, IndexerError> {
        self.spawn_blocking(move |this| this.get_event(id)).await
    }

    async fn get_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> Result<EventPage, IndexerError> {
        self.spawn_blocking(move |this| this.get_events(query, cursor, limit, descending_order))
            .await
    }

    async fn get_object(
        &self,
        object_id: ObjectID,
        version: Option<SequenceNumber>,
    ) -> Result<ObjectRead, IndexerError> {
        self.spawn_blocking(move |this| this.get_object(object_id, version))
            .await
    }

    async fn query_objects_history(
        &self,
        filter: SuiObjectDataFilter,
        at_checkpoint: CheckpointSequenceNumber,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<ObjectRead>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.query_objects_history(filter, at_checkpoint, cursor, limit)
        })
        .await
    }

    async fn query_latest_objects(
        &self,
        filter: SuiObjectDataFilter,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<ObjectRead>, IndexerError> {
        self.spawn_blocking(move |this| this.query_latest_objects(filter, cursor, limit))
            .await
    }

    async fn get_total_transaction_number_from_checkpoints(&self) -> Result<i64, IndexerError> {
        self.spawn_blocking(move |this| this.get_total_transaction_number_from_checkpoints())
            .await
    }

    async fn get_transaction_by_digest(
        &self,
        tx_digest: &str,
    ) -> Result<Transaction, IndexerError> {
        let tx_digest = tx_digest.to_owned();
        self.spawn_blocking(move |this| this.get_transaction_by_digest(&tx_digest))
            .await
    }

    async fn multi_get_transactions_by_digests(
        &self,
        tx_digests: &[String],
    ) -> Result<Vec<Transaction>, IndexerError> {
        let tx_digests = tx_digests.to_owned();
        self.spawn_blocking(move |this| this.multi_get_transactions_by_digests(&tx_digests))
            .await
    }

    async fn compose_sui_transaction_block_response(
        &self,
        tx: Transaction,
        options: Option<&SuiTransactionBlockResponseOptions>,
    ) -> Result<SuiTransactionBlockResponse, IndexerError> {
        let options = options.cloned();
        self.spawn_blocking(move |this| {
            this.compose_sui_transaction_block_response(tx, options.as_ref())
        })
        .await
    }

    async fn get_all_transaction_page(
        &self,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_all_transaction_page(start_sequence, limit, is_descending)
        })
        .await
    }

    async fn get_transaction_page_by_checkpoint(
        &self,
        checkpoint_sequence_number: i64,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_transaction_page_by_checkpoint(
                checkpoint_sequence_number,
                start_sequence,
                limit,
                is_descending,
            )
        })
        .await
    }

    async fn get_transaction_page_by_transaction_kinds(
        &self,
        kind_names: Vec<String>,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_transaction_page_by_transaction_kinds(
                kind_names,
                start_sequence,
                limit,
                is_descending,
            )
        })
        .await
    }

    async fn get_transaction_page_by_sender_address(
        &self,
        sender_address: String,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_transaction_page_by_sender_address(
                sender_address,
                start_sequence,
                limit,
                is_descending,
            )
        })
        .await
    }

    async fn get_transaction_page_by_recipient_address(
        &self,
        sender_address: Option<SuiAddress>,
        recipient_address: SuiAddress,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_transaction_page_by_recipient_address(
                sender_address,
                recipient_address,
                start_sequence,
                limit,
                is_descending,
            )
        })
        .await
    }

    async fn get_transaction_page_by_address(
        &self,
        address: SuiAddress,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_transaction_page_by_address(address, start_sequence, limit, is_descending)
        })
        .await
    }

    async fn get_transaction_page_by_input_object(
        &self,
        object_id: ObjectID,
        version: Option<i64>,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_transaction_page_by_input_object(
                object_id,
                version,
                start_sequence,
                limit,
                is_descending,
            )
        })
        .await
    }

    async fn get_transaction_page_by_changed_object(
        &self,
        object_id: ObjectID,
        version: Option<i64>,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_transaction_page_by_changed_object(
                object_id,
                version,
                start_sequence,
                limit,
                is_descending,
            )
        })
        .await
    }

    async fn get_transaction_page_by_move_call(
        &self,
        package: ObjectID,
        module: Option<Identifier>,
        function: Option<Identifier>,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_transaction_page_by_move_call(
                package,
                module,
                function,
                start_sequence,
                limit,
                is_descending,
            )
        })
        .await
    }

    async fn get_transaction_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_transaction_sequence_by_digest(tx_digest, is_descending)
        })
        .await
    }

    async fn get_move_call_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_move_call_sequence_by_digest(tx_digest, is_descending)
        })
        .await
    }

    async fn get_input_object_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_input_object_sequence_by_digest(tx_digest, is_descending)
        })
        .await
    }

    async fn get_changed_object_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_changed_object_sequence_by_digest(tx_digest, is_descending)
        })
        .await
    }

    async fn get_recipient_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        self.spawn_blocking(move |this| {
            this.get_recipient_sequence_by_digest(tx_digest, is_descending)
        })
        .await
    }

    async fn get_network_metrics(&self) -> Result<NetworkMetrics, IndexerError> {
        self.spawn_blocking(move |this| this.get_network_metrics())
            .await
    }

    async fn get_move_call_metrics(&self) -> Result<MoveCallMetrics, IndexerError> {
        self.spawn_blocking(move |this| this.get_move_call_metrics())
            .await
    }

    async fn persist_fast_path(
        &self,
        tx: Transaction,
        tx_object_changes: TransactionObjectChanges,
    ) -> Result<usize, IndexerError> {
        self.spawn_blocking(move |this| this.persist_fast_path(tx, tx_object_changes))
            .await
    }

    async fn persist_checkpoint_transactions(
        &self,
        checkpoint: &Checkpoint,
        transactions: &[Transaction],
        total_transaction_chunk_committed_counter: IntCounter,
    ) -> Result<usize, IndexerError> {
        let checkpoint = checkpoint.to_owned();
        let transactions = transactions.to_owned();
        self.spawn_blocking(move |this| {
            this.persist_checkpoint_transactions(
                &checkpoint,
                &transactions,
                total_transaction_chunk_committed_counter,
            )
        })
        .await
    }

    async fn persist_object_changes(
        &self,
        tx_object_changes: &[TransactionObjectChanges],
        total_object_change_chunk_committed_counter: IntCounter,
        object_mutation_latency: Histogram,
        object_deletion_latency: Histogram,
    ) -> Result<(), IndexerError> {
        let tx_object_changes = tx_object_changes.to_owned();
        self.spawn_blocking(move |this| {
            this.persist_object_changes(
                &tx_object_changes,
                total_object_change_chunk_committed_counter,
                object_mutation_latency,
                object_deletion_latency,
            )
        })
        .await
    }

    async fn persist_events(&self, events: &[Event]) -> Result<(), IndexerError> {
        let events = events.to_owned();
        self.spawn_blocking(move |this| this.persist_events(&events))
            .await
    }

    async fn persist_addresses(
        &self,
        addresses: &[Address],
        active_addresses: &[ActiveAddress],
    ) -> Result<(), IndexerError> {
        let addresses = addresses.to_owned();
        let active_addresses = active_addresses.to_owned();
        self.spawn_blocking(move |this| this.persist_addresses(&addresses, &active_addresses))
            .await
    }

    async fn persist_packages(&self, packages: &[Package]) -> Result<(), IndexerError> {
        let packages = packages.to_owned();
        self.spawn_blocking(move |this| this.persist_packages(&packages))
            .await
    }

    async fn persist_transaction_index_tables(
        &self,
        input_objects: &[InputObject],
        changed_objects: &[ChangedObject],
        move_calls: &[MoveCall],
        recipients: &[Recipient],
    ) -> Result<(), IndexerError> {
        let input_objects = input_objects.to_owned();
        let changed_objects = changed_objects.to_owned();
        let move_calls = move_calls.to_owned();
        let recipients = recipients.to_owned();
        self.spawn_blocking(move |this| {
            this.persist_transaction_index_tables(
                &input_objects,
                &changed_objects,
                &move_calls,
                &recipients,
            )
        })
        .await
    }

    async fn persist_epoch(&self, data: &TemporaryEpochStore) -> Result<(), IndexerError> {
        let data = data.to_owned();
        self.spawn_blocking(move |this| this.persist_epoch(&data))
            .await
    }

    async fn get_network_total_transactions_previous_epoch(
        &self,
        epoch: i64,
    ) -> Result<i64, IndexerError> {
        self.spawn_blocking(move |this| this.get_network_total_transactions_previous_epoch(epoch))
            .await
    }

    async fn get_epochs(
        &self,
        cursor: Option<EpochId>,
        limit: usize,
        descending_order: Option<bool>,
    ) -> Result<Vec<EpochInfo>, IndexerError> {
        self.spawn_blocking(move |this| this.get_epochs(cursor, limit, descending_order))
            .await
    }

    async fn get_current_epoch(&self) -> Result<EpochInfo, IndexerError> {
        self.spawn_blocking(move |this| this.get_current_epoch())
            .await
    }

    fn module_cache(&self) -> &Self::ModuleCache {
        &self.module_cache
    }

    fn indexer_metrics(&self) -> &IndexerMetrics {
        &self.metrics
    }

    async fn get_last_address_processed_checkpoint(&self) -> Result<i64, IndexerError> {
        self.spawn_blocking(move |this| this.get_last_address_processed_checkpoint())
            .await
    }

    async fn calculate_address_stats(&self, checkpoint: i64) -> Result<AddressStats, IndexerError> {
        self.spawn_blocking(move |this| this.calculate_address_stats(checkpoint))
            .await
    }

    async fn persist_address_stats(&self, addr_stats: &AddressStats) -> Result<(), IndexerError> {
        let addr_stats = addr_stats.to_owned();
        self.spawn_blocking(move |this| this.persist_address_stats(&addr_stats))
            .await
    }

    async fn get_latest_address_stats(&self) -> Result<AddressStats, IndexerError> {
        self.spawn_blocking(move |this| this.get_latest_address_stats())
            .await
    }

    async fn get_checkpoint_address_stats(
        &self,
        checkpoint: i64,
    ) -> Result<AddressStats, IndexerError> {
        self.spawn_blocking(move |this| this.get_checkpoint_address_stats(checkpoint))
            .await
    }

    async fn get_all_epoch_address_stats(
        &self,
        descending_order: Option<bool>,
    ) -> Result<Vec<AddressStats>, IndexerError> {
        self.spawn_blocking(move |this| this.get_all_epoch_address_stats(descending_order))
            .await
    }

    async fn calculate_checkpoint_metrics(
        &self,
        current_checkpoint: i64,
        last_checkpoint_metrics: &CheckpointMetrics,
        checkpoints: &[Checkpoint],
    ) -> Result<CheckpointMetrics, IndexerError> {
        let last_checkpoint_metrics = last_checkpoint_metrics.to_owned();
        let checkpoints = checkpoints.to_owned();
        self.spawn_blocking(move |this| {
            this.calculate_checkpoint_metrics(
                current_checkpoint,
                &last_checkpoint_metrics,
                &checkpoints,
            )
        })
        .await
    }

    async fn persist_checkpoint_metrics(
        &self,
        checkpoint_metrics: &CheckpointMetrics,
    ) -> Result<(), IndexerError> {
        let checkpoint_metrics = checkpoint_metrics.to_owned();
        self.spawn_blocking(move |this| this.persist_checkpoint_metrics(&checkpoint_metrics))
            .await
    }

    async fn get_latest_checkpoint_metrics(&self) -> Result<CheckpointMetrics, IndexerError> {
        self.spawn_blocking(move |this| this.get_latest_checkpoint_metrics())
            .await
    }

    async fn calculate_real_time_tps(&self, current_checkpoint: i64) -> Result<f64, IndexerError> {
        self.spawn_blocking(move |this| this.calculate_real_time_tps(current_checkpoint))
            .await
    }

    async fn calculate_peak_tps_30d(
        &self,
        current_checkpoint: i64,
        current_timestamp_ms: i64,
    ) -> Result<f64, IndexerError> {
        self.spawn_blocking(move |this| {
            this.calculate_peak_tps_30d(current_checkpoint, current_timestamp_ms)
        })
        .await
    }
}

fn persist_transaction_object_changes(
    conn: &mut PgConnection,
    mutated_objects: Vec<Object>,
    deleted_objects: Vec<Object>,
    total_object_change_chunk_committed_counter: Option<IntCounter>,
    object_mutation_latency: Option<Histogram>,
    object_deletion_latency: Option<Histogram>,
) -> Result<usize, IndexerError> {
    // TODO(gegaowp): tx object changes from one tx do not need group_and_sort_objects, will optimize soon after this PR.
    // NOTE: to avoid error of `ON CONFLICT DO UPDATE command cannot affect row a second time`,
    // we have to limit update of one object once in a query.

    // MUSTFIX(gegaowp): clean up the metrics codes after experiment
    if let (Some(object_mutation_latency), Some(object_change_chunk_counter)) = (
        object_mutation_latency,
        total_object_change_chunk_committed_counter.clone(),
    ) {
        let object_mutation_guard = object_mutation_latency.start_timer();
        let mut mutated_object_groups = group_and_sort_objects(mutated_objects);
        let mut db_commit_counter = 0;
        loop {
            let mutated_object_group = mutated_object_groups
                .iter_mut()
                .filter_map(|group| group.pop())
                .collect::<Vec<_>>();
            if mutated_object_group.is_empty() {
                break;
            }
            // bulk insert/update via UNNEST trick
            let insert_update_query =
                compose_object_bulk_insert_update_query(&mutated_object_group);
            diesel::sql_query(insert_update_query)
                .execute(conn)
                .map_err(|e| {
                    IndexerError::PostgresWriteError(format!(
                        "Failed writing mutated objects to PostgresDB with error: {:?}",
                        e
                    ))
                })?;
            db_commit_counter += 1;
        }
        object_mutation_guard.stop_and_record();
        object_change_chunk_counter.inc_by(db_commit_counter);
    } else {
        let mut mutated_object_groups = group_and_sort_objects(mutated_objects);
        loop {
            let mutated_object_group = mutated_object_groups
                .iter_mut()
                .filter_map(|group| group.pop())
                .collect::<Vec<_>>();
            if mutated_object_group.is_empty() {
                break;
            }
            // bulk insert/update via UNNEST trick
            let insert_update_query =
                compose_object_bulk_insert_update_query(&mutated_object_group);
            diesel::sql_query(insert_update_query)
                .execute(conn)
                .map_err(|e| {
                    IndexerError::PostgresWriteError(format!(
                        "Failed writing mutated objects to PostgresDB with error: {:?}",
                        e
                    ))
                })?;
        }
    }

    // MUSTFIX(gegaowp): clean up the metrics codes after experiment
    if let (Some(object_deletion_latency), Some(object_deletion_chunk_counter)) = (
        object_deletion_latency,
        total_object_change_chunk_committed_counter,
    ) {
        let mut db_commit_counter = 0;
        let object_deletion_guard = object_deletion_latency.start_timer();
        for deleted_object_change_chunk in deleted_objects.chunks(PG_COMMIT_CHUNK_SIZE) {
            diesel::insert_into(objects::table)
                .values(deleted_object_change_chunk)
                .on_conflict(objects::object_id)
                .do_update()
                .set((
                    objects::epoch.eq(excluded(objects::epoch)),
                    objects::checkpoint.eq(excluded(objects::checkpoint)),
                    objects::version.eq(excluded(objects::version)),
                    objects::previous_transaction.eq(excluded(objects::previous_transaction)),
                    objects::object_status.eq(excluded(objects::object_status)),
                ))
                .execute(conn)
                .map_err(|e| {
                    IndexerError::PostgresWriteError(format!(
                        "Failed writing deleted objects to PostgresDB with error: {:?}",
                        e
                    ))
                })?;
            db_commit_counter += 1;
        }
        object_deletion_guard.stop_and_record();
        object_deletion_chunk_counter.inc_by(db_commit_counter);
    } else {
        for deleted_object_change_chunk in deleted_objects.chunks(PG_COMMIT_CHUNK_SIZE) {
            diesel::insert_into(objects::table)
                .values(deleted_object_change_chunk)
                .on_conflict(objects::object_id)
                .do_update()
                .set((
                    objects::epoch.eq(excluded(objects::epoch)),
                    objects::checkpoint.eq(excluded(objects::checkpoint)),
                    objects::version.eq(excluded(objects::version)),
                    objects::previous_transaction.eq(excluded(objects::previous_transaction)),
                    objects::object_status.eq(excluded(objects::object_status)),
                ))
                .execute(conn)
                .map_err(|e| {
                    IndexerError::PostgresWriteError(format!(
                        "Failed writing deleted objects to PostgresDB with error: {:?}",
                        e
                    ))
                })?;
        }
    }
    Ok(0)
}

#[derive(Clone)]
struct PartitionManager {
    cp: PgConnectionPool,
}

impl PartitionManager {
    fn new(cp: PgConnectionPool) -> Result<Self, IndexerError> {
        // Find all tables with partition
        let manager = Self { cp };
        let tables = manager.get_table_partitions()?;
        info!(
            "Found {} tables with partitions : [{:?}]",
            tables.len(),
            tables
        );
        Ok(manager)
    }

    // MUSTFIX(gegaowp): temporarily disable partition management.
    #[allow(dead_code)]
    fn advance_epoch(
        &self,
        new_epoch: &DBEpochInfo,
        last_epoch_start_cp: i64,
    ) -> Result<(), IndexerError> {
        let next_epoch_id = new_epoch.epoch;
        let last_epoch_id = new_epoch.epoch - 1;
        let next_epoch_start_cp = new_epoch.first_checkpoint_id;

        let tables = self.get_table_partitions()?;

        let table_updated = transactional_blocking!(&self.cp, |conn| {
            let mut updated_table = vec![];
            for (table, last_partition) in &tables {
                if last_partition < &(next_epoch_id as u64) {
                    let detach_partition = format!(
                        "ALTER TABLE {table} DETACH PARTITION {table}_partition_{last_epoch_id};"
                    );
                    let attach_partition_with_new_range = format!("ALTER TABLE {table} ATTACH PARTITION {table}_partition_{last_epoch_id} FOR VALUES FROM ('{last_epoch_start_cp}') TO ('{next_epoch_start_cp}');");
                    info! {"Changed last epoch partition {last_epoch_id} for {table}, with new range {last_epoch_start_cp} to {next_epoch_start_cp}"};
                    let new_partition = format!("CREATE TABLE {table}_partition_{next_epoch_id} PARTITION OF {table} FOR VALUES FROM ({next_epoch_start_cp}) TO (MAXVALUE);");
                    info! {"Created epoch partition {next_epoch_id} for {table}, with new range {next_epoch_start_cp} to MAXVALUE"};
                    diesel::RunQueryDsl::execute(diesel::sql_query(detach_partition), conn)?;
                    diesel::RunQueryDsl::execute(
                        diesel::sql_query(attach_partition_with_new_range),
                        conn,
                    )?;
                    diesel::RunQueryDsl::execute(diesel::sql_query(new_partition), conn)?;
                    updated_table.push(table.clone());
                }
            }
            Ok::<_, diesel::result::Error>(updated_table)
        })?;
        info! {"Created epoch partition {next_epoch_id} for {table_updated:?}"};
        Ok(())
    }

    fn get_table_partitions(&self) -> Result<BTreeMap<String, u64>, IndexerError> {
        #[derive(QueryableByName, Debug, Clone)]
        struct PartitionedTable {
            #[diesel(sql_type = VarChar)]
            table_name: String,
            #[diesel(sql_type = VarChar)]
            last_partition: String,
        }

        Ok(
            read_only_blocking!(&self.cp, |conn| diesel::RunQueryDsl::load(
                diesel::sql_query(GET_PARTITION_SQL),
                conn
            ))?
            .into_iter()
            .map(|table: PartitionedTable| {
                u64::from_str(&table.last_partition)
                    .map(|last_partition| (table.table_name, last_partition))
                    .map_err(|e| anyhow!(e))
            })
            .collect::<Result<_, _>>()?,
        )
    }
}

// Run this function only once every `time` seconds
#[once(time = 60, sync_writes = true, result = true)]
fn get_network_metrics_cached(cp: &PgConnectionPool) -> Result<NetworkMetrics, IndexerError> {
    let metrics = read_only_blocking!(cp, |conn| diesel::sql_query(
        "SELECT * FROM network_metrics;"
    )
    .get_result::<DBNetworkMetrics>(conn))?;
    Ok(metrics.into())
}
