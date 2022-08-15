// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! SQL and SQLite-based Event Store

use super::*;

use async_trait::async_trait;
use serde_json::Value;
use sqlx::ConnectOptions;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use strum::{EnumMessage, IntoEnumIterator};
use sui_types::base_types::{SequenceNumber, SuiAddress};
use sui_types::object::Owner;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteRow},
    Executor, Row, SqlitePool,
};
use sui_types::error::SuiError;
use sui_types::event::{Event, TransferType, TransferTypeVariants};
use tracing::{debug, info, log, warn};

/// Maximum number of events one can ask for right now
const MAX_LIMIT: usize = 5000;

/// Sqlite-based Event Store
///
/// ## Data Model
/// - Main columns hold most common fields
/// - object_id is used for multiple purposes, including the Publish package ID
/// - event_type is an integer in order to save space and corresponds to EventType discriminant
/// - fields is JSON for now (for easy JSON filtering) and contains all fields not in main columns
pub struct SqlEventStore {
    pool: SqlitePool,
    // Sequence number is used to prevent previously ingested events from being ingested again
    // It acts as a cache, as the seq_num field is also written to the DB.
    seq_num: AtomicU64,
}

/// Important for updating Columns:
/// 1. put the SQL CREATE TABLE line for each field in the comments
/// 2. to add a new column, append to the enum list for backward compatibility
/// 3. update `SQL_INSERT_TX` accordingly
/// This is some strum macros magic so we can programmatically get the column number / position,
/// and generate consistent tables as well.
#[derive(strum_macros::EnumMessage, strum_macros::EnumIter)]
#[repr(u8)]
enum EventsTableColumns {
    /// timestamp INTEGER NOT NULL
    Timestamp = 0,
    /// seq_num INTEGER
    SeqNum,
    /// checkpoint INTEGER
    Checkpoint,
    /// tx_digest BLOB
    TxDigest,
    /// event_type INTEGER
    EventType,
    /// package_id BLOB
    PackageId,
    /// module_name TEXT
    ModuleName,
    /// function TEXT
    Function,
    /// object_id BLOB
    ObjectId,
    /// fields TEXT
    Fields,
    /// type_struct TEXT
    TypeStruct,
    /// contents BLOB
    Contents,
    /// sender BLOB
    Sender,
    /// recipient TEXT
    Recipient,
    /// object_version INTEGER
    ObjectVersion,
    /// transfer_type INTEGER
    TransferType,
}

const SQL_INSERT_TX: &str =
    "INSERT INTO events (timestamp, seq_num, checkpoint, tx_digest, event_type, \
    package_id, module_name, object_id, fields, type_struct, contents, sender,  \
    recipient, object_version, transfer_type) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

const INDEXED_COLUMNS: &[&str] = &[
    "timestamp",
    "tx_digest",
    "event_type",
    "package_id",
    "module_name",
];

impl SqlEventStore {
    /// Creates a new SQLite in-memory database, mostly for testing
    pub async fn new_memory_only_not_prod() -> Result<Self, SuiError> {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .map_err(convert_sqlx_err)?;
        info!("Created new in-memory SQLite EventStore for testing");
        Ok(Self {
            pool,
            seq_num: AtomicU64::new(0),
        })
    }

    /// Creates or opens a new SQLite database at a specific path
    pub async fn new_from_file(db_path: &Path) -> Result<Self, SuiError> {
        // TODO: configure other SQLite options
        let mut options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true);
        options.log_statements(log::LevelFilter::Off);
        let pool = SqlitePool::connect_with(options)
            .await
            .map_err(convert_sqlx_err)?;
        info!(?db_path, "Created/opened SQLite EventStore on disk");
        Ok(Self {
            pool,
            seq_num: AtomicU64::new(0),
        })
    }

    /// Initializes the database, creating tables and indexes as needed
    /// It should be safe to call this every time after new_sqlite() as IF NOT EXISTS are used.
    pub async fn initialize(&self) -> Result<(), SuiError> {
        // First create the table if needed... make the create out of the enum for consistency
        // NOTE: If the below line errors, docstring might be missing for a field
        let table_columns: Vec<_> = EventsTableColumns::iter()
            .map(|c| c.get_documentation().unwrap())
            .collect();
        let create_sql = format!(
            "CREATE TABLE IF NOT EXISTS events({});",
            table_columns.join(", ")
        );
        self.pool
            .execute(create_sql.as_str())
            .await
            .map_err(convert_sqlx_err)?;
        info!("SQLite events table is initialized with query {create_sql:?}");

        // Then, create indexes
        for column in INDEXED_COLUMNS {
            // NOTE: Cannot prepare CREATE INDEX statements.
            // Also, this may take a long time if we add fields to index, at startup.  TODO
            self.pool
                .execute(
                    format!(
                        "CREATE INDEX IF NOT EXISTS {}_idx on events ({})",
                        column, column
                    )
                    .as_str(),
                )
                .await
                .map_err(convert_sqlx_err)?;
            info!(column, "Index is ready");
        }

        // Setting last sequence number
        let last_seq_num = self.last_seq_num().await?;
        self.seq_num.store(last_seq_num, Ordering::Relaxed);
        info!(
            last_seq_num,
            "Recovered last sequence number from event store"
        );

        Ok(())
    }

    /// Returns total size of table.  Should really only be used for testing.
    #[allow(unused)]
    async fn total_event_count(&self) -> Result<usize, SuiError> {
        let result = sqlx::query("SELECT COUNT(*) FROM events")
            .fetch_one(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        let num_rows: i64 = result.get(0);
        Ok(num_rows as usize)
    }

    async fn last_seq_num(&self) -> Result<u64, SuiError> {
        let result = sqlx::query("SELECT MAX(seq_num) FROM events")
            .fetch_one(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        let num_rows: i64 = result.get(0);
        Ok(num_rows as u64)
    }

    fn try_extract_object_id(row: &SqliteRow, col: usize) -> Result<Option<ObjectID>, SuiError> {
        let raw_bytes: Option<Vec<u8>> = row.get(col);
        match raw_bytes {
            Some(bytes) => Ok(Some(ObjectID::try_from(bytes.as_slice()).map_err(
                |_e| SuiError::BadObjectType {
                    error: format!("Could not parse bytes {:?} into ObjectID", bytes),
                },
            )?)),
            None => Ok(None),
        }
    }

    fn try_extract_transfer_type(row: &SqliteRow) -> Result<Option<TransferType>, SuiError> {
        let ordinal: Option<u16> = row.get(EventsTableColumns::TransferType as usize);
        match ordinal {
            Some(ordinal) => Ok(Some(Event::transfer_type_from_ordinal(ordinal as usize)?)),
            None => Ok(None),
        }
    }

    fn try_extract_object_version(row: &SqliteRow) -> Option<SequenceNumber> {
        // sqlite does not support u64 so we store it as i64
        let index = EventsTableColumns::ObjectVersion as usize;
        let object_version: Option<i64> = row.get(index);
        object_version.map(|ov| SequenceNumber::from_u64(ov as u64))
    }

    fn try_extract_sender_address(row: &SqliteRow) -> Result<Option<SuiAddress>, SuiError> {
        let raw_bytes: Option<Vec<u8>> = row.get(EventsTableColumns::Sender as usize);
        match raw_bytes {
            Some(bytes) => Ok(Some(SuiAddress::try_from(bytes.as_slice()).map_err(
                |_e| SuiError::BadObjectType {
                    error: format!(
                        "Could not parse sender address bytes: {:?} into SuiAddress",
                        bytes
                    ),
                },
            )?)),
            None => Ok(None),
        }
    }

    fn try_extract_recipient(row: &SqliteRow) -> Result<Option<Owner>, SuiError> {
        let deserialized_owner: Option<String> = row.get(EventsTableColumns::Recipient as usize);

        match deserialized_owner {
            Some(owner_string) => Ok(Some(serde_json::from_str::<Owner>(&owner_string).map_err(
                |_e| SuiError::BadObjectType {
                    error: format!(
                        "Could not deserialize owner str: {:?} into Owner",
                        owner_string
                    ),
                },
            )?)),
            None => Ok(None),
        }
    }

    // Adds JSON fields for items not in any of the standard columns in table definition, eg for MOVE events.
    fn event_to_json(event: &EventEnvelope) -> String {
        if let Some(json_value) = &event.move_struct_json_value {
            json_value.to_string()
        } else {
            String::new()
        }
    }

    fn check_limit(limit: usize) -> Result<(), SuiError> {
        if limit <= MAX_LIMIT {
            Ok(())
        } else {
            Err(SuiError::TooManyItemsError(MAX_LIMIT as u64))
        }
    }
}

impl From<SqliteRow> for StoredEvent {
    // Translate a Row into StoredEvent
    // TODO: convert to use FromRow trait so query_as() could be used?
    // TODO: gracefully handle data corruption/incompatibility without panicking
    fn from(row: SqliteRow) -> Self {
        let timestamp: i64 = row.get(EventsTableColumns::Timestamp as usize);
        let checkpoint: i64 = row.get(EventsTableColumns::Checkpoint as usize);
        let digest_raw: Option<Vec<u8>> = row.get(EventsTableColumns::TxDigest as usize);
        let tx_digest = digest_raw.map(|bytes| {
            TransactionDigest::new(
                bytes
                    .try_into()
                    .expect("Cannot convert digest bytes to TxDigest"),
            )
        });
        let event_type: u16 = row.get(EventsTableColumns::EventType as usize);
        let package_id =
            SqlEventStore::try_extract_object_id(&row, EventsTableColumns::PackageId as usize)
                .expect("Error converting stored package ID bytes to ObjectID");
        let object_id =
            SqlEventStore::try_extract_object_id(&row, EventsTableColumns::ObjectId as usize)
                .expect("Error converting stored object ID bytes to ObjectID");
        let module_name: Option<String> = row.get(EventsTableColumns::ModuleName as usize);
        let function: Option<String> = row.get(EventsTableColumns::Function as usize);
        let fields_text: &str = row.get(EventsTableColumns::Fields as usize);
        let fields: Vec<_> = if fields_text.is_empty() {
            Vec::new()
        } else {
            let fields_json = serde_json::from_str(fields_text)
                .unwrap_or_else(|e| panic!("Could not parse [{}] as JSON: {}", fields_text, e));
            if let Value::Object(map) = fields_json {
                map.into_iter()
                    .map(|(k, v)| (flexstr::SharedStr::from(k), EventValue::Json(v)))
                    .collect()
            } else {
                warn!(
                    ?fields_json,
                    "Could not parse JSON as object, should not happen"
                );
                Vec::new()
            }
        };
        let move_event_contents: Option<Vec<u8>> = row.get(EventsTableColumns::Contents as usize);
        let move_event_struct_tag: Option<String> =
            row.get(EventsTableColumns::TypeStruct as usize);
        let sender = SqlEventStore::try_extract_sender_address(&row)
            .expect("Error converting stored sender address bytes to SuiAddress");
        let recipient = SqlEventStore::try_extract_recipient(&row)
            .expect("Error converting stored recipient address to Owner");
        let object_version = SqlEventStore::try_extract_object_version(&row);
        let transfer_type = SqlEventStore::try_extract_transfer_type(&row)
            .expect("Error converting stored transfer type to TransferType");

        StoredEvent {
            timestamp: timestamp as u64,
            checkpoint_num: checkpoint as u64,
            tx_digest,
            event_type: SharedStr::from(Event::name_from_ordinal(event_type as usize)),
            package_id,
            module_name: module_name.map(|s| s.into()),
            function_name: function.map(SharedStr::from),
            object_id,
            fields,
            move_event_contents,
            move_event_struct_tag,
            sender,
            recipient,
            object_version,
            transfer_type,
        }
    }
}

const TS_QUERY: &str = "SELECT * FROM events WHERE timestamp >= ? AND timestamp < ? LIMIT ?";

const TX_QUERY: &str = "SELECT * FROM events WHERE tx_digest = ?";

// TODO: do we really need `DESC`?
const QUERY_BY_TYPE: &str = "SELECT * FROM events WHERE timestamp >= ? AND \
    timestamp < ? AND event_type = ? ORDER BY timestamp DESC LIMIT ?";

// TODO: do we really need `DESC`?
const QUERY_BY_MODULE: &str = "SELECT * FROM events WHERE timestamp >= ? AND \
    timestamp < ? AND package_id = ? AND module_name = ? ORDER BY timestamp DESC LIMIT ?";

const QUERY_BY_CHECKPOINT: &str = "SELECT * FROM events WHERE checkpoint >= ? AND checkpoint < ?";

#[async_trait]
impl EventStore for SqlEventStore {
    async fn add_events(
        &self,
        events: &[EventEnvelope],
        checkpoint_num: u64,
    ) -> Result<u64, SuiError> {
        // TODO: submit writes in one transaction/batch so it won't just fail in the middle
        let mut cur_seq = self.seq_num.load(Ordering::Acquire);
        let initial_seq = cur_seq;
        let mut rows_affected: u64 = 0;

        // TODO: benchmark
        // TODO: use techniques in https://docs.rs/sqlx-core/0.5.13/sqlx_core/query_builder/struct.QueryBuilder.html#method.push_values
        // to execute all inserts in a single statement?
        // TODO: See https://kerkour.com/high-performance-rust-with-sqlite
        for event in events {
            // Skip events that have a lower sequence number... which must be same or increasing
            if event.seq_num < cur_seq {
                debug!(tx_digest =? event.tx_digest, seq_num = event.seq_num, cur_seq, "Skipping event with lower sequence number than current");
                continue;
            }
            cur_seq = event.seq_num;

            // If batching, turn off persistent to avoid caching as we may fill up the prepared statement cache
            let insert_tx_q = sqlx::query(SQL_INSERT_TX).persistent(true);
            let event_type = EventType::from(&event.event);

            let transfer_type = event
                .event
                .transfer_type()
                .map(|tt| TransferTypeVariants::from(tt) as u16);

            // sqlite does not support u64, so we have to cast it to i64 before storing
            let object_version = event.event.object_version().map(|ov| ov.value() as i64);

            let sender = event.event.sender().map(|sender| sender.to_vec());

            // TODO: use batched API?
            let q = insert_tx_q
                .bind(event.timestamp as i64)
                .bind(event.seq_num as i64)
                .bind(checkpoint_num as i64)
                .bind(event.tx_digest.map(|txd| txd.to_bytes()))
                .bind(event_type as u16)
                .bind(event.event.package_id().map(|pid| pid.to_vec()))
                .bind(event.event.module_name())
                .bind(event.event.object_id().map(|id| id.to_vec()))
                .bind(Self::event_to_json(event))
                .bind(event.event.move_event_struct_tag().map(|st| st.to_string()))
                .bind(event.event.move_event_contents())
                .bind(sender)
                .bind(event.event.recipient_serialized()?)
                .bind(object_version)
                .bind(transfer_type);
            let res = q.execute(&self.pool).await.map_err(convert_sqlx_err)?;
            rows_affected += res.rows_affected();
        }

        // CAS is used to detect any concurrency glitches.  Note that we assume a single writer
        // append model, which is currently true.  In single writer the CAS should never fail.
        // We also do this after writing all events, for efficiency.
        if cur_seq > initial_seq {
            self.seq_num
                .compare_exchange(initial_seq, cur_seq, Ordering::Acquire, Ordering::Relaxed)
                .expect("CAS Failure - event writes are not single threaded");
        }

        Ok(rows_affected)
    }

    async fn events_for_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let rows = sqlx::query(TX_QUERY)
            .persistent(true)
            .bind(digest.to_bytes())
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }

    async fn events_by_type(
        &self,
        start_time: u64,
        end_time: u64,
        event_type: EventType,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        Self::check_limit(limit)?;
        let rows = sqlx::query(QUERY_BY_TYPE)
            .persistent(true)
            .bind(start_time as i64)
            .bind(end_time as i64)
            .bind(event_type as u16)
            .bind(limit as i64)
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }

    async fn event_iterator(
        &self,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        Self::check_limit(limit)?;
        let rows = sqlx::query(TS_QUERY)
            .bind(start_time as i64)
            .bind(end_time as i64)
            .bind(limit as i64)
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }

    fn events_by_checkpoint(
        &self,
        start_checkpoint: u64,
        end_checkpoint: u64,
    ) -> Result<StreamedResult, SuiError> {
        let stream = sqlx::query(QUERY_BY_CHECKPOINT)
            .bind(start_checkpoint as i64)
            .bind(end_checkpoint as i64)
            .map(StoredEvent::from)
            .fetch(&self.pool)
            .map(|r| r.map_err(convert_sqlx_err));
        Ok(StreamedResult::new(Box::pin(stream)))
    }

    async fn events_by_module_id(
        &self,
        start_time: u64,
        end_time: u64,
        module: ModuleId,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        Self::check_limit(limit)?;
        let rows = sqlx::query(QUERY_BY_MODULE)
            .persistent(true)
            .bind(start_time as i64)
            .bind(end_time as i64)
            .bind(module.address().to_vec())
            .bind(module.name().to_string())
            .bind(limit as i64)
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }
}

fn convert_sqlx_err(err: sqlx::Error) -> SuiError {
    SuiError::GenericStorageError(err.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::test_utils;
    use super::*;
    use flexstr::shared_str;
    use move_core_types::{account_address::AccountAddress, identifier::Identifier};

    use sui_types::event::{EventEnvelope, TransferType};

    fn test_queried_event_vs_test_envelope(
        queried: &StoredEvent,
        orig: &EventEnvelope,
        checkpoint: u64,
    ) {
        assert_eq!(queried.timestamp, orig.timestamp);
        assert_eq!(queried.checkpoint_num, checkpoint);
        assert_eq!(queried.tx_digest, orig.tx_digest);
        assert_eq!(queried.event_type, shared_str!(orig.event_type()));
        assert_eq!(queried.package_id, orig.event.package_id());
        assert_eq!(
            queried.module_name,
            orig.event.module_name().map(SharedStr::from)
        );
        assert_eq!(queried.object_id, orig.event.object_id());
        assert_eq!(queried.sender, orig.event.sender());
        assert_eq!(queried.recipient.as_ref(), orig.event.recipient());
        assert_eq!(queried.transfer_type.as_ref(), orig.event.transfer_type());
        assert_eq!(queried.object_version.as_ref(), orig.event.object_version());
        assert_eq!(
            queried.move_event_contents.as_deref(),
            orig.event.move_event_contents()
        );
        let move_event_struct_tag = orig
            .event
            .move_event_struct_tag()
            .map(|tag| tag.to_string());
        assert_eq!(
            queried.move_event_struct_tag.as_ref(),
            move_event_struct_tag.as_ref()
        );
    }

    #[tokio::test]
    async fn test_eventstore_basic_insert_read() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        // Initialize store
        let db = SqlEventStore::new_memory_only_not_prod().await?;
        db.initialize().await?;

        // Insert some records
        info!("Inserting records!");
        let to_insert = vec![
            test_utils::new_test_newobj_event(1_000_000, 1),
            test_utils::new_test_publish_event(1_001_000, 2),
            test_utils::new_test_transfer_event(1_002_000, 3, TransferType::Coin),
            test_utils::new_test_deleteobj_event(1_003_000, 3),
            test_utils::new_test_transfer_event(1_004_000, 4, TransferType::ToAddress),
            test_utils::new_test_move_event(
                1_005_000,
                5,
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
            ),
        ];
        assert_eq!(db.add_events(&to_insert, 1).await?, 6);
        info!("Done inserting");

        assert_eq!(db.total_event_count().await?, 6);

        // Query for records in time range, end should be exclusive - should get 2
        let queried_events = db.event_iterator(1_000_000, 1_002_000, 20).await?;
        assert_eq!(queried_events.len(), 2);
        for i in 0..2 {
            test_queried_event_vs_test_envelope(&queried_events[i], &to_insert[i], 1);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_eventstore_query_by_checkpoint() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        let db = SqlEventStore::new_memory_only_not_prod().await?;
        db.initialize().await?;

        // Insert txns from checkpoint 1
        info!("Inserting records!");
        let checkpoint_1 = vec![
            test_utils::new_test_newobj_event(1_000_000, 1),
            test_utils::new_test_newobj_event(1_001_000, 2),
            test_utils::new_test_newobj_event(1_002_000, 3),
        ];
        let mut tx_digests_checkpoint1 = checkpoint_1
            .iter()
            .map(|e| e.tx_digest.unwrap())
            .collect::<HashSet<TransactionDigest>>();
        assert_eq!(db.add_events(&checkpoint_1, 1).await?, 3);
        info!("Done inserting");
        assert_eq!(db.total_event_count().await?, 3);

        // Query txns between checkpoint [1, 3), expect 3 txns from checkpoint 1
        let mut event_stream = db.events_by_checkpoint(1, 3)?;
        let events = event_stream.next_chunk(100).await?;
        let tx_digests = events
            .iter()
            .map(|e| e.tx_digest.unwrap())
            .collect::<HashSet<TransactionDigest>>();
        assert_eq!(tx_digests, tx_digests_checkpoint1);

        // Insert txns from checkpoint 2
        let checkpoint_2 = vec![
            test_utils::new_test_deleteobj_event(1_003_000, 3),
            test_utils::new_test_deleteobj_event(1_004_000, 4),
            test_utils::new_test_deleteobj_event(1_005_000, 5),
        ];
        let tx_digests_checkpoint2 = checkpoint_2
            .iter()
            .map(|e| e.tx_digest.unwrap())
            .collect::<HashSet<TransactionDigest>>();
        assert_eq!(db.add_events(&checkpoint_2, 2).await?, 3);
        assert_eq!(db.total_event_count().await?, 6);

        // Query txns between checkpoint [2, 3), expect 3 txns from checkpoint 2
        let mut event_stream = db.events_by_checkpoint(2, 3)?;
        let events = event_stream.next_chunk(100).await?;
        let tx_digests = events
            .iter()
            .map(|e| e.tx_digest.unwrap())
            .collect::<HashSet<TransactionDigest>>();
        assert_eq!(tx_digests, tx_digests_checkpoint2);

        // Query txns between checkpoint [1, 3), expect 6 txns from checkpoint 1 and 2
        let mut event_stream = db.events_by_checkpoint(1, 3)?;
        let events = event_stream.next_chunk(100).await?;
        let tx_digests = events
            .iter()
            .map(|e| e.tx_digest.unwrap())
            .collect::<HashSet<TransactionDigest>>();
        tx_digests_checkpoint1.extend(tx_digests_checkpoint2);
        assert_eq!(tx_digests, tx_digests_checkpoint1);

        Ok(())
    }

    #[tokio::test]
    async fn test_eventstore_transfers_tx_read() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        // Initialize store
        let db = SqlEventStore::new_memory_only_not_prod().await?;
        db.initialize().await?;

        // Insert some records
        info!("Inserting records!");
        let to_insert = vec![
            test_utils::new_test_newobj_event(1_000_000, 1),
            test_utils::new_test_publish_event(1_001_000, 2),
            test_utils::new_test_transfer_event(1_002_000, 3, TransferType::Coin),
            test_utils::new_test_deleteobj_event(1_003_000, 3),
            test_utils::new_test_transfer_event(1_004_000, 4, TransferType::ToAddress),
            test_utils::new_test_move_event(
                1_005_000,
                5,
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
            ),
        ];
        db.add_events(&to_insert, 1).await?;
        let target_event = &to_insert[2];
        info!("Done inserting");

        // Query for transfer event
        let mut events = db
            .events_for_transaction(target_event.tx_digest.unwrap())
            .await?;
        assert_eq!(events.len(), 1); // Should be no more events, just that one
        let transfer_event = events.pop().unwrap();

        test_queried_event_vs_test_envelope(&transfer_event, target_event, 1);

        // expect empty fields
        assert_eq!(transfer_event.fields.len(), 0);

        Ok(())
    }

    // Test for reads by event type, plus returning events in desc timestamp and limit
    #[tokio::test]
    async fn test_eventstore_query_by_type() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        // Initialize store
        let db = SqlEventStore::new_memory_only_not_prod().await?;
        db.initialize().await?;

        // Insert some records
        info!("Inserting records!");
        let to_insert = vec![
            test_utils::new_test_newobj_event(1_000_000, 1),
            test_utils::new_test_publish_event(1_001_000, 2),
            test_utils::new_test_transfer_event(1_002_000, 3, TransferType::Coin),
            test_utils::new_test_deleteobj_event(1_003_000, 3),
            test_utils::new_test_transfer_event(1_004_000, 4, TransferType::ToAddress),
            test_utils::new_test_move_event(
                1_005_000,
                5,
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
            ),
        ];
        db.add_events(&to_insert, 1).await?;
        info!("Done inserting");

        let queried_events = db
            .events_by_type(1_000_000, 1_005_000, EventType::TransferObject, 2)
            .await?;
        assert_eq!(queried_events.len(), 2);

        // Desc timestamp order, so the last transfer event should be first
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[4], 1);
        test_queried_event_vs_test_envelope(&queried_events[1], &to_insert[2], 1);

        // Query again with limit of 1, it should return only the last transfer event
        let queried_events = db
            .events_by_type(1_000_000, 1_005_000, EventType::TransferObject, 1)
            .await?;
        assert_eq!(queried_events.len(), 1);
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[4], 1);

        // Query with wrong time range, return 0 events
        let queried_events = db
            .events_by_type(1_006_000, 1_009_000, EventType::TransferObject, 1)
            .await?;
        assert_eq!(queried_events.len(), 0);

        // Query Publish Event
        let queried_events = db
            .events_by_type(1_001_000, 1_002_000, EventType::Publish, 1)
            .await?;
        assert_eq!(queried_events.len(), 1);
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[1], 1);

        // Query NewObject Event
        let queried_events = db
            .events_by_type(1_000_000, 1_002_000, EventType::NewObject, 1)
            .await?;
        assert_eq!(queried_events.len(), 1);
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[0], 1);

        // Query DeleteObject Event
        let queried_events = db
            .events_by_type(1_003_000, 1_004_000, EventType::DeleteObject, 1)
            .await?;
        assert_eq!(queried_events.len(), 1);
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[3], 1);

        // Query Move Event
        let queried_events = db
            .events_by_type(1_004_000, 1_006_000, EventType::MoveEvent, 1)
            .await?;
        assert_eq!(queried_events.len(), 1);
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[5], 1);

        Ok(())
    }

    // Test for reads by move event
    #[tokio::test]
    async fn test_eventstore_move_events() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        // Initialize store
        let db = SqlEventStore::new_memory_only_not_prod().await?;
        db.initialize().await?;

        // Insert some records
        info!("Inserting records!");
        let to_insert = vec![
            test_utils::new_test_newobj_event(1_000_000, 1),
            test_utils::new_test_publish_event(1_001_000, 2),
            test_utils::new_test_transfer_event(1_002_000, 3, TransferType::Coin),
            test_utils::new_test_deleteobj_event(1_003_000, 3),
            test_utils::new_test_transfer_event(1_004_000, 4, TransferType::ToAddress),
            test_utils::new_test_move_event(
                1_005_000,
                5,
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
            ),
            test_utils::new_test_move_event(
                1_006_000,
                6,
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
            ),
        ];
        db.add_events(&to_insert, 1).await?;
        info!("Done inserting");

        // Query for the Move event and validate basic fields
        let events = db
            .events_for_transaction(to_insert[5].tx_digest.unwrap())
            .await?;
        let move_event = &events[0];
        assert_eq!(events.len(), 1); // Should be no more events, just that one

        test_queried_event_vs_test_envelope(move_event, &to_insert[5], 1);
        assert_eq!(move_event.fields.len(), 2);

        // Query by module ID
        let mod_id = ModuleId::new(
            AccountAddress::from(ObjectID::from_hex_literal("0x3").unwrap()),
            Identifier::from_str("test_module").unwrap(),
        );
        let queried_events = db
            .events_by_module_id(1_000_000, 1_006_001, mod_id, 3)
            .await?;
        assert_eq!(queried_events.len(), 2);

        // results are sorted in DESC order
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[6], 1);
        test_queried_event_vs_test_envelope(&queried_events[1], &to_insert[5], 1);
        assert_eq!(queried_events[0].fields.len(), 2);
        assert_eq!(queried_events[1].fields.len(), 2);

        Ok(())
    }

    // Test creating and opening file-based database
    #[tokio::test]
    async fn test_eventstore_max_limit() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        // Initialize store
        let dir = tempfile::TempDir::new().unwrap();
        let db = SqlEventStore::new_from_file(&dir.path().join("events.db")).await?;
        db.initialize().await?;

        let res = db.event_iterator(1_000_000, 1_002_000, 100_000).await;
        assert!(matches!(res, Err(SuiError::TooManyItemsError(_))));

        Ok(())
    }

    // Test Idempotency / Sequence Numbering
    #[tokio::test]
    async fn test_eventstore_seq_num() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        // Initialize store
        let dir = tempfile::TempDir::new().unwrap(); // NOTE this must be its own line so dir isn't dropped
        let db_file = dir.path().join("events.db");
        let db = SqlEventStore::new_from_file(&db_file).await?;
        db.initialize().await?;

        // Write in some events, all should succeed
        let to_insert = vec![
            test_utils::new_test_newobj_event(1_000_000, 1),
            test_utils::new_test_publish_event(1_001_000, 2),
            test_utils::new_test_transfer_event(1_002_000, 3, TransferType::Coin),
            test_utils::new_test_deleteobj_event(1_003_000, 3),
            test_utils::new_test_transfer_event(1_004_000, 4, TransferType::ToAddress),
            test_utils::new_test_move_event(
                1_005_000,
                5,
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
            ),
        ];
        assert_eq!(db.add_events(&to_insert[..4], 1).await?, 4);
        assert_eq!(db.total_event_count().await?, 4);

        // Write in an older event with older sequence number, should be skipped
        assert_eq!(db.add_events(&to_insert[1..2], 1).await?, 0);
        assert_eq!(db.total_event_count().await?, 4);

        // Drop and reload DB from the same file, test that sequence number was recovered
        drop(db);
        let db = SqlEventStore::new_from_file(&db_file).await?;
        db.initialize().await?;
        assert_eq!(db.last_seq_num().await?, 3);
        assert_eq!(db.total_event_count().await?, 4);

        // Try ingesting older event, check still skipped
        assert_eq!(db.add_events(&to_insert[1..2], 1).await?, 0);
        assert_eq!(db.total_event_count().await?, 4);

        // Check writing new events still succeeds
        assert_eq!(db.add_events(&to_insert[4..], 1).await?, 2);
        assert_eq!(db.total_event_count().await?, 6);

        Ok(())
    }
}
