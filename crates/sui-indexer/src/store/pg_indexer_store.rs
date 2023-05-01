// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use cached::proc_macro::once;
use diesel::dsl::max;
use diesel::pg::PgConnection;
use diesel::query_builder::AsQuery;
use diesel::sql_types::{BigInt, VarChar};
use diesel::upsert::excluded;
use diesel::{ExpressionMethods, PgArrayExpressionMethods};
use diesel::{OptionalExtension, QueryableByName};
use diesel::{QueryDsl, RunQueryDsl};
use fastcrypto::hash::Digest;
use fastcrypto::traits::ToFromBytes;
use move_bytecode_utils::module_cache::SyncModuleCache;
use move_core_types::identifier::Identifier;
use prometheus::Histogram;
use tracing::info;

use sui_json_rpc::{ObjectProvider, ObjectProviderCache};
use sui_json_rpc_types::{
    CheckpointId, EpochInfo, EventFilter, EventPage, MoveCallMetrics, MoveFunctionName,
    NetworkMetrics, SuiEvent, SuiObjectDataFilter,
};
use sui_json_rpc_types::{
    SuiTransactionBlock, SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockEvents, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
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

use crate::errors::{Context, IndexerError};
use crate::metrics::IndexerMetrics;
use crate::models::addresses::Address;
use crate::models::checkpoints::Checkpoint;
use crate::models::epoch::DBEpochInfo;
use crate::models::events::Event;
use crate::models::network_metrics::{DBMoveCallMetrics, DBNetworkMetrics};
use crate::models::objects::{
    compose_object_bulk_insert_update_query, group_and_sort_objects, Object,
};
use crate::models::packages::Package;
use crate::models::system_state::DBValidatorSummary;
use crate::models::transaction_index::{InputObject, MoveCall, Recipient};
use crate::models::transactions::Transaction;
use crate::schema::{
    addresses, checkpoints, checkpoints::dsl as checkpoints_dsl, epochs, epochs::dsl as epochs_dsl,
    events, input_objects, input_objects::dsl as input_objects_dsl, move_calls,
    move_calls::dsl as move_calls_dsl, objects, objects::dsl as objects_dsl, objects_history,
    packages, recipients, recipients::dsl as recipients_dsl, system_states, transactions,
    transactions::dsl as transactions_dsl, validators,
};
use crate::store::diesel_marco::{read_only_blocking, transactional_blocking};
use crate::store::indexer_store::TemporaryCheckpointStore;
use crate::store::module_resolver::IndexerModuleResolver;
use crate::store::query::DBFilter;
use crate::store::TransactionObjectChanges;
use crate::store::{IndexerStore, TemporaryEpochStore};
use crate::utils::{get_balance_changes_from_effect, get_object_changes};
use crate::{AsyncPgConnectionPool, PgConnectionPool};

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
    #[allow(dead_code)]
    cp: AsyncPgConnectionPool,
    blocking_cp: PgConnectionPool,
    // MUSTFIX(gegaowp): temporarily disable partition management.
    #[allow(dead_code)]
    partition_manager: PartitionManager,
    module_cache: Arc<SyncModuleCache<IndexerModuleResolver>>,
    metrics: IndexerMetrics,
}

impl PgIndexerStore {
    pub async fn new(
        cp: AsyncPgConnectionPool,
        blocking_cp: PgConnectionPool,
        metrics: IndexerMetrics,
    ) -> Self {
        let module_cache = Arc::new(SyncModuleCache::new(IndexerModuleResolver::new(
            blocking_cp.clone(),
        )));
        PgIndexerStore {
            cp: cp.clone(),
            blocking_cp: blocking_cp.clone(),
            partition_manager: PartitionManager::new(blocking_cp).await.unwrap(),
            module_cache,
            metrics,
        }
    }

    pub async fn get_sui_types_object(
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

    pub async fn find_sui_types_object_lt_or_eq_version(
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
}

#[async_trait]
impl IndexerStore for PgIndexerStore {
    type ModuleCache = SyncModuleCache<IndexerModuleResolver>;

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<i64, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints_dsl::checkpoints
                .select(max(checkpoints::sequence_number))
                .first::<Option<i64>>(conn)
                // -1 to differentiate between no checkpoints and the first checkpoint
                .map(|o| o.unwrap_or(-1))
        })
        .context("Failed reading latest checkpoint sequence number from PostgresDB")
    }

    async fn get_latest_object_checkpoint_sequence_number(&self) -> Result<i64, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            objects::dsl::objects
                .select(max(objects::checkpoint))
                .first::<Option<i64>>(conn)
                // -1 to differentiate between no checkpoints and the first checkpoint
                .map(|o| o.unwrap_or(-1))
        })
        .context("Failed reading latest object checkpoint sequence number from PostgresDB")
    }

    async fn get_checkpoint(
        &self,
        id: CheckpointId,
    ) -> Result<sui_json_rpc_types::Checkpoint, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let cp: Checkpoint = match id {
                CheckpointId::SequenceNumber(seq) => checkpoints_dsl::checkpoints
                    .filter(checkpoints::sequence_number.eq(seq as i64))
                    .limit(1)
                    .first(conn),
                CheckpointId::Digest(digest) => checkpoints_dsl::checkpoints
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
        .context(
            format!(
                "Failed reading previous checkpoint {:?} from PostgresDB",
                id
            )
            .as_str(),
        )
    }

    async fn get_checkpoint_sequence_number(
        &self,
        digest: CheckpointDigest,
    ) -> Result<CheckpointSequenceNumber, IndexerError> {
        Ok(
            read_only_blocking!(&self.blocking_cp, |conn| checkpoints_dsl::checkpoints
                .select(checkpoints::sequence_number)
                .filter(checkpoints::checkpoint_digest.eq(digest.base58_encode()))
                .first::<i64>(conn))
            .context("Failed reading checkpoint seq number from PostgresDB")? as u64,
        )
    }

    async fn get_event(&self, id: EventID) -> Result<Event, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| events::table
            .filter(events::dsl::transaction_digest.eq(id.tx_digest.base58_encode()))
            .filter(events::dsl::event_sequence.eq(id.event_seq as i64))
            .first::<Event>(conn))
        .context("Failed reading event from PostgresDB")
    }

    async fn get_events(
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
                Some(self.get_event(cursor).await?.id.ok_or_else(|| {
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

    async fn get_total_transaction_number_from_checkpoints(&self) -> Result<i64, IndexerError> {
        let checkpoint: Checkpoint = read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints_dsl::checkpoints
                .order(checkpoints_dsl::network_total_transactions.desc())
                .first::<Checkpoint>(conn)
        })
        .context("Failed reading total transaction number")?;
        Ok(checkpoint.network_total_transactions)
    }

    async fn get_transaction_by_digest(
        &self,
        tx_digest: &str,
    ) -> Result<Transaction, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            transactions_dsl::transactions
                .filter(transactions_dsl::transaction_digest.eq(tx_digest))
                .first::<Transaction>(conn)
        })
        .context(&format!(
            "Failed reading transaction with digest {tx_digest}"
        ))
    }

    async fn compose_sui_transaction_block_response(
        &self,
        tx: Transaction,
        options: Option<&SuiTransactionBlockResponseOptions>,
    ) -> Result<SuiTransactionBlockResponse, IndexerError> {
        let transaction: SuiTransactionBlock =
            serde_json::from_str(&tx.transaction_content).map_err(|err| {
                IndexerError::InsertableParsingError(format!(
                    "Failed converting transaction JSON {:?} to SuiTransactionBlock with error: {:?}",
                    tx.transaction_content, err
                ))
            })?;
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
        let sender = SuiAddress::from_str(tx.sender.as_str())?;

        let (mut tx_opt, mut effects_opt, mut raw_tx) = (None, None, vec![]);
        let (mut object_changes, mut balance_changes, mut events) = (None, None, None);
        if let Some(options) = options {
            if options.show_balance_changes {
                let object_cache = ObjectProviderCache::new(self.clone());
                balance_changes =
                    Some(get_balance_changes_from_effect(&object_cache, &effects).await?);
            }
            if options.show_object_changes {
                let object_cache = ObjectProviderCache::new(self.clone());
                object_changes = Some(
                    get_object_changes(
                        &object_cache,
                        sender,
                        &effects.modified_at_versions(),
                        effects.all_changed_objects(),
                        effects.all_deleted_objects(),
                    )
                    .await?,
                );
            }
            if options.show_events {
                let event_page = self
                    .get_events(
                        EventFilter::Transaction(tx_digest),
                        None,
                        None,
                        /* descending_order */ false,
                    )
                    .await?;
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

    async fn multi_get_transactions_by_digests(
        &self,
        tx_digests: &[String],
    ) -> Result<Vec<Transaction>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            transactions_dsl::transactions
                .filter(transactions_dsl::transaction_digest.eq_any(tx_digests))
                .load::<Transaction>(conn)
        })
        .context(&format!(
            "Failed reading transactions with digests {tx_digests:?}"
        ))
    }

    async fn get_transaction_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            if let Some(digest) = &tx_digest {
                let mut boxed_query = transactions_dsl::transactions
                    .filter(transactions_dsl::transaction_digest.eq(digest))
                    .select(transactions_dsl::id)
                    .into_boxed();
                if is_descending {
                    boxed_query = boxed_query.order(transactions_dsl::id.desc());
                } else {
                    boxed_query = boxed_query.order(transactions_dsl::id.asc());
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

    async fn get_object(
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
                objects_dsl::objects
                    .filter(objects_dsl::object_id.eq(object_id.to_string()))
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

    async fn query_objects_history(
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
    async fn query_latest_objects(
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

    async fn get_move_call_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            if let Some(digest) = &tx_digest {
                let mut boxed_query = move_calls_dsl::move_calls
                    .filter(move_calls_dsl::transaction_digest.eq(digest))
                    .select(move_calls_dsl::id)
                    .into_boxed();
                if is_descending {
                    boxed_query = boxed_query.order(move_calls_dsl::id.desc());
                } else {
                    boxed_query = boxed_query.order(move_calls_dsl::id.asc());
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

    async fn get_input_object_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            if let Some(digest) = &tx_digest {
                let mut boxed_query = input_objects_dsl::input_objects
                    .filter(input_objects_dsl::transaction_digest.eq(digest))
                    .select(input_objects_dsl::id)
                    .into_boxed();
                if is_descending {
                    boxed_query = boxed_query.order(input_objects_dsl::id.desc());
                } else {
                    boxed_query = boxed_query.order(input_objects_dsl::id.asc());
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

    async fn get_recipient_sequence_by_digest(
        &self,
        tx_digest: Option<String>,
        is_descending: bool,
    ) -> Result<Option<i64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            if let Some(digest) = &tx_digest {
                let mut boxed_query = recipients_dsl::recipients
                    .filter(recipients_dsl::transaction_digest.eq(digest))
                    .select(recipients_dsl::id)
                    .into_boxed();
                if is_descending {
                    boxed_query = boxed_query.order(recipients_dsl::id.desc());
                } else {
                    boxed_query = boxed_query.order(recipients_dsl::id.asc());
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

    async fn get_all_transaction_page(
        &self,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let mut boxed_query = transactions_dsl::transactions.into_boxed();
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    boxed_query = boxed_query.filter(transactions_dsl::id.lt(start_sequence));
                } else {
                    boxed_query = boxed_query.filter(transactions_dsl::id.gt(start_sequence));
                }
            }

            if is_descending {
                boxed_query
                    .order(transactions_dsl::id.desc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            } else {
                boxed_query
                    .order(transactions_dsl::id.asc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            }
        }).context(&format!("Failed reading all transaction digests with start_sequence {start_sequence:?} and limit {limit}"))
    }

    async fn get_transaction_page_by_checkpoint(
        &self,
        checkpoint_sequence_number: i64,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let mut boxed_query = transactions_dsl::transactions
                .filter(transactions_dsl::checkpoint_sequence_number.eq(checkpoint_sequence_number))
                .into_boxed();
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    boxed_query = boxed_query.filter(transactions_dsl::id.lt(start_sequence));
                } else {
                    boxed_query = boxed_query.filter(transactions_dsl::id.gt(start_sequence));
                }
            }
            if is_descending {
                boxed_query
                    .order(transactions_dsl::id.desc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            } else {
                boxed_query
                    .order(transactions_dsl::id.asc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            }
        }).context(&format!("Failed reading transaction digests with checkpoint_sequence_number {checkpoint_sequence_number:?} and start_sequence {start_sequence:?} and limit {limit}"))
    }

    async fn get_transaction_page_by_transaction_kind(
        &self,
        kind: String,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let mut boxed_query = transactions_dsl::transactions
                .filter(transactions_dsl::transaction_kind.eq(kind.clone()))
                .into_boxed();
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    boxed_query = boxed_query.filter(transactions_dsl::id.lt(start_sequence));
                } else {
                    boxed_query = boxed_query.filter(transactions_dsl::id.gt(start_sequence));
                }
            }

            if is_descending {
                boxed_query
                    .order(transactions_dsl::id.desc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            } else {
               boxed_query
                    .order(transactions_dsl::id.asc())
                    .limit((limit) as i64)
                    .load::<Transaction>(conn)
            }
        }).context(&format!("Failed reading transaction digests with kind {kind} and start_sequence {start_sequence:?} and limit {limit}"))
    }

    async fn get_transaction_page_by_mutated_object(
        &self,
        object_id: String,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let mut boxed_query = transactions_dsl::transactions
                .filter(transactions_dsl::mutated.contains(vec![Some(object_id.clone())]))
                .or_filter(transactions_dsl::created.contains(vec![Some(object_id.clone())]))
                .or_filter(transactions_dsl::unwrapped.contains(vec![Some(object_id.clone())]))
                .into_boxed();
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    boxed_query = boxed_query.filter(transactions_dsl::id.lt(start_sequence));
                } else {
                    boxed_query = boxed_query.filter(transactions_dsl::id.gt(start_sequence));
                }
            }
            if is_descending {
               boxed_query
                    .order(transactions_dsl::id.desc())
                    .limit(limit as i64)
                    .load::<Transaction>(conn)
            } else {
                boxed_query
                    .order(transactions_dsl::id.asc())
                    .limit(limit as i64)
                    .load::<Transaction>(conn)
            }
        }).context(&format!("Failed reading transaction digests by mutated object id {object_id} with start_sequence {start_sequence:?} and limit {limit}"))
    }

    async fn get_transaction_page_by_sender_address(
        &self,
        sender_address: String,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            let mut boxed_query = transactions_dsl::transactions
                .filter(transactions_dsl::sender.eq(sender_address.clone()))
                .into_boxed();
            if let Some(start_sequence) = start_sequence {
                if is_descending {
                    boxed_query = boxed_query.filter(transactions_dsl::id.lt(start_sequence));
                } else {
                    boxed_query = boxed_query.filter(transactions_dsl::id.gt(start_sequence));
                }
            }

            if is_descending {
                boxed_query
                    .order(transactions_dsl::id.desc())
                    .limit(limit as i64)
                    .load::<Transaction>(conn)
            } else {
               boxed_query
                    .order(transactions_dsl::id.asc())
                    .limit(limit as i64)
                    .load::<Transaction>(conn)
            }
        }).context(&format!("Failed reading transaction digests by sender address {sender_address} with start_sequence {start_sequence:?} and limit {limit}"))
    }

    async fn get_transaction_page_by_input_object(
        &self,
        object_id: ObjectID,
        version: Option<i64>,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        let sql_query = format!(
            "SELECT transaction_digest as digest_name FROM (
                SELECT transaction_digest, max(id) AS max_id
                FROM input_objects
                WHERE object_id = '{}' {} {}
                GROUP BY transaction_digest
                ORDER BY max_id {} LIMIT {}
            ) AS t",
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
        self.multi_get_transactions_by_digests(&tx_digests).await
    }

    async fn get_transaction_page_by_move_call(
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
            "SELECT transaction_digest as digest_name FROM (
                SELECT transaction_digest, max(id) AS max_id
                FROM move_calls
                WHERE move_package = '{}' {} {} {}
                GROUP BY transaction_digest
                ORDER BY max_id {} LIMIT {}
            ) AS t",
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
        self.multi_get_transactions_by_digests(&tx_digests).await
    }

    async fn get_transaction_page_by_sender_recipient_address(
        &self,
        from: Option<SuiAddress>,
        to: SuiAddress,
        start_sequence: Option<i64>,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<Transaction>, IndexerError> {
        let sql_query = format!(
            "SELECT transaction_digest as digest_name FROM (
                SELECT transaction_digest, max(id) AS max_id
                FROM recipients
                WHERE recipient = '{}' {} {} GROUP BY transaction_digest
                ORDER BY max_id {} LIMIT {}
            ) AS t",
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
        self.multi_get_transactions_by_digests(&tx_digests).await
    }

    async fn get_network_metrics(&self) -> Result<NetworkMetrics, IndexerError> {
        get_network_metrics_cached(&self.blocking_cp).await
    }

    async fn get_move_call_metrics(&self) -> Result<MoveCallMetrics, IndexerError> {
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

    async fn persist_fast_path(
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
            )
        })
    }

    fn persist_all_checkpoint_data(
        &self,
        data: &TemporaryCheckpointStore,
    ) -> Result<usize, IndexerError> {
        let TemporaryCheckpointStore {
            checkpoint,
            transactions,
            events,
            object_changes: tx_object_changes,
            addresses,
            packages,
            input_objects,
            move_calls,
            recipients,
        } = data;

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

            // Commit indexed events
            for event_chunk in events.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(events::table)
                    .values(event_chunk)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing events to PostgresDB")?;
            }

            // Commit indexed objects
            let mutated_objects: Vec<Object> = tx_object_changes
                .iter()
                .flat_map(|changes| changes.changed_objects.iter().cloned())
                .collect();
            // TODO(gegaowp): monitor the deletion batch size to see
            // if bulk update via unnest is necessary.
            let deleted_changes = tx_object_changes
                .iter()
                .flat_map(|changes| changes.deleted_objects.iter().cloned())
                .collect::<Vec<_>>();
            let deleted_objects: Vec<Object> = deleted_changes
                .iter()
                .map(|deleted_object| deleted_object.clone().into())
                .collect();
            persist_transaction_object_changes(conn, mutated_objects, deleted_objects, None, None)?;

            // Commit indexed addresses
            for addresses_chunk in addresses.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(addresses::table)
                    .values(addresses_chunk)
                    .on_conflict(addresses::account_address)
                    .do_nothing()
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing addresses to PostgresDB")?;
            }

            // Commit indexed packages
            for packages_chunk in packages.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(packages::table)
                    .values(packages_chunk)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing packages to PostgresDB")?;
            }

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

            // Commit indexed recipients
            for recipients_chunk in recipients.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(recipients::table)
                    .values(recipients_chunk)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing recipients to PostgresDB")?;
            }

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

    async fn persist_checkpoint_transactions(
        &self,
        checkpoint: &Checkpoint,
        transactions: &[Transaction],
    ) -> Result<usize, IndexerError> {
        let trimmed_transactions = transactions
            .iter()
            .map(|t| {
                let mut t = t.clone();
                t.trim_data();
                t
            })
            .collect::<Vec<_>>();
        transactional_blocking!(&self.blocking_cp, |conn| {
            // Commit indexed transactions
            for transaction_chunk in trimmed_transactions.chunks(PG_COMMIT_CHUNK_SIZE) {
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

    async fn persist_object_changes(
        &self,
        checkpoint: &Checkpoint,
        tx_object_changes: &[TransactionObjectChanges],
        object_mutation_latency: Histogram,
        object_deletion_latency: Histogram,
    ) -> Result<(), IndexerError> {
        transactional_blocking!(&self.blocking_cp, |conn| {
            // update epoch transaction count
            let sql = "UPDATE epochs e1
SET epoch_total_transactions = e2.epoch_total_transactions + $1
FROM epochs e2
WHERE e1.epoch = e2.epoch
  AND e1.epoch = $2;";
            diesel::sql_query(sql)
                .bind::<BigInt, _>(checkpoint.transactions.len() as i64)
                .bind::<BigInt, _>(checkpoint.epoch)
                .as_query()
                .execute(conn)?;

            {
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
                let (mutation_count, deletion_count) =
                    (mutated_objects.len(), deleted_objects.len());

                // MUSTFIX(gegaowp): trim data to reduce short-term storage consumption.
                let trimmed_mutated_objects = mutated_objects
                    .into_iter()
                    .map(|mut o| {
                        o.trim_data();
                        o
                    })
                    .collect::<Vec<Object>>();
                let trimmed_deleted_objects = deleted_objects
                    .into_iter()
                    .map(|mut o| {
                        o.trim_data();
                        o
                    })
                    .collect::<Vec<Object>>();
                persist_transaction_object_changes(
                    conn,
                    trimmed_mutated_objects,
                    trimmed_deleted_objects,
                    Some(object_mutation_latency),
                    Some(object_deletion_latency),
                )?;
                info!(
                "Object checkpoint {} committed with {} transaction, {} mutated objects and {} deleted objects.",
                checkpoint.sequence_number,
                tx_object_changes.len(),
                mutation_count,
                deletion_count
            );
                Ok::<(), IndexerError>(())
            }
        })?;
        Ok(())
    }

    async fn persist_events(&self, events: &[Event]) -> Result<(), IndexerError> {
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

    async fn persist_addresses(&self, addresses: &[Address]) -> Result<(), IndexerError> {
        transactional_blocking!(&self.blocking_cp, |conn| {
            for addresses_chunk in addresses.chunks(PG_COMMIT_CHUNK_SIZE) {
                diesel::insert_into(addresses::table)
                    .values(addresses_chunk)
                    .on_conflict(addresses::account_address)
                    .do_nothing()
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed writing addresses to PostgresDB")?;
            }
            Ok::<(), IndexerError>(())
        })?;
        Ok(())
    }
    async fn persist_packages(&self, packages: &[Package]) -> Result<(), IndexerError> {
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

    async fn persist_transaction_index_tables(
        &self,
        input_objects: &[InputObject],
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

    async fn persist_epoch(&self, data: &TemporaryEpochStore) -> Result<(), IndexerError> {
        // MUSTFIX(gegaowp): temporarily disable the epoch advance logic.
        // let last_epoch_cp_id = if data.last_epoch.is_none() {
        //     0
        // } else {
        //     self.get_current_epoch().await?.first_checkpoint_id as i64
        // };
        if data.last_epoch.is_none() {
            let last_epoch_cp_id = 0;
            self.partition_manager
                .advance_epoch(&data.new_epoch, last_epoch_cp_id)
                .await?;
        }
        let epoch = data.new_epoch.epoch;
        info!("Persisting epoch {}", epoch);

        transactional_blocking!(&self.blocking_cp, |conn| {
            if let Some(last_epoch) = &data.last_epoch {
                diesel::insert_into(epochs::table)
                    .values(last_epoch)
                    .on_conflict(epochs::epoch)
                    .do_update()
                    .set((
                        epochs::last_checkpoint_id.eq(excluded(epochs::last_checkpoint_id)),
                        epochs::epoch_end_timestamp.eq(excluded(epochs::epoch_end_timestamp)),
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
            }
            diesel::insert_into(epochs::table)
                .values(&data.new_epoch)
                .on_conflict_do_nothing()
                .execute(conn)?;

            diesel::insert_into(system_states::table)
                .values(&data.system_state)
                .on_conflict_do_nothing()
                .execute(conn)?;

            diesel::insert_into(validators::table)
                .values(&data.validators)
                .on_conflict_do_nothing()
                .execute(conn)
        })?;
        info!("Persisted epoch {}", epoch);
        Ok(())
    }

    fn module_cache(&self) -> &Self::ModuleCache {
        &self.module_cache
    }

    fn indexer_metrics(&self) -> &IndexerMetrics {
        &self.metrics
    }

    async fn get_epochs(
        &self,
        cursor: Option<EpochId>,
        limit: usize,
        descending_order: Option<bool>,
    ) -> Result<Vec<EpochInfo>, IndexerError> {
        let is_descending = descending_order.unwrap_or_default();
        let id = cursor
            .map(|id| id as i64)
            .unwrap_or(if is_descending { i64::MAX } else { -1 });
        let mut query = epochs_dsl::epochs.into_boxed();
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

    async fn get_current_epoch(&self) -> Result<EpochInfo, IndexerError> {
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
}

fn persist_transaction_object_changes(
    conn: &mut PgConnection,
    mutated_objects: Vec<Object>,
    deleted_objects: Vec<Object>,
    object_mutation_latency: Option<Histogram>,
    object_deletion_latency: Option<Histogram>,
) -> Result<usize, IndexerError> {
    // TODO(gegaowp): tx object changes from one tx do not need group_and_sort_objects, will optimize soon after this PR.
    // NOTE: to avoid error of `ON CONFLICT DO UPDATE command cannot affect row a second time`,
    // we have to limit update of one object once in a query.

    // MUSTFIX(gegaowp): clean up the metrics codes after experiment
    if let Some(object_mutation_latency) = object_mutation_latency {
        let object_mutation_guard = object_mutation_latency.start_timer();
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
        object_mutation_guard.stop_and_record();
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
    if let Some(object_deletion_latency) = object_deletion_latency {
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
        }
        object_deletion_guard.stop_and_record();
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
    async fn new(cp: PgConnectionPool) -> Result<Self, IndexerError> {
        // Find all tables with partition
        let manager = Self { cp };
        let tables = manager.get_table_partitions().await?;
        info!(
            "Found {} tables with partitions : [{:?}]",
            tables.len(),
            tables
        );
        Ok(manager)
    }

    // MUSTFIX(gegaowp): temporarily disable partition management.
    #[allow(dead_code)]
    async fn advance_epoch(
        &self,
        new_epoch: &DBEpochInfo,
        last_epoch_start_cp: i64,
    ) -> Result<(), IndexerError> {
        let next_epoch_id = new_epoch.epoch;
        let last_epoch_id = new_epoch.epoch - 1;
        let next_epoch_start_cp = new_epoch.first_checkpoint_id;

        let tables = self.get_table_partitions().await?;

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

    async fn get_table_partitions(&self) -> Result<BTreeMap<String, u64>, IndexerError> {
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

#[once(time = 2, result = true)]
async fn get_network_metrics_cached(cp: &PgConnectionPool) -> Result<NetworkMetrics, IndexerError> {
    let metrics = read_only_blocking!(cp, |conn| diesel::sql_query(
        "SELECT * FROM network_metrics;"
    )
    .get_result::<DBNetworkMetrics>(conn))?;
    Ok(metrics.into())
}

#[async_trait]
impl ObjectProvider for PgIndexerStore {
    type Error = IndexerError;
    async fn get_object(
        &self,
        _id: &ObjectID,
        _version: &SequenceNumber,
    ) -> Result<sui_types::object::Object, Self::Error> {
        Err(IndexerError::NotSupportedError(
            "Object cache is not supported".to_string(),
        ))
        // self.get_sui_types_object(id, version)
    }

    async fn find_object_lt_or_eq_version(
        &self,
        _id: &ObjectID,
        _version: &SequenceNumber,
    ) -> Result<Option<sui_types::object::Object>, Self::Error> {
        Err(IndexerError::NotSupportedError(
            "find_object_lt_or_eq_version is not supported".to_string(),
        ))
        // self.find_sui_types_object_lt_or_eq_version(id, version)
        //     .await
    }
}
