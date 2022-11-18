// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! SQL and SQLite-based Event Store

use core::time::Duration;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::path::Path;

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::ConnectOptions;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteRow, SqliteSynchronous},
    Executor, QueryBuilder, Row, SqlitePool,
};
use strum::{EnumMessage, IntoEnumIterator};
use tracing::{info, instrument, log, warn};

use sui_types::base_types::SuiAddress;
use sui_types::error::SuiError;
use sui_types::event::Event;
use sui_types::object::Owner;

use super::*;

/// Sqlite-based Event Store
///
/// ## Data Model
/// - Main columns hold most common fields
/// - object_id is used for multiple purposes, including the Publish package ID
/// - event_type is an integer in order to save space and corresponds to EventType discriminant
/// - fields is JSON for now (for easy JSON filtering) and contains all fields not in main columns
pub struct SqlEventStore {
    pool: SqlitePool,
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
    /// event_num INTEGER
    EventNum,
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
    /// object_type TEXT
    ObjectType,
    /// object_id BLOB
    ObjectId,
    /// fields TEXT
    Fields,
    /// move_event_name TEXT
    MoveEventName,
    /// contents BLOB
    Contents,
    /// sender BLOB
    Sender,
    /// recipient TEXT
    Recipient,
}

const SQL_INSERT_TX: &str =
    "INSERT OR IGNORE INTO events (timestamp, seq_num, event_num, tx_digest, event_type, \
    package_id, module_name, object_id, object_type, fields, move_event_name, contents, sender,  \
    recipient) ";

const INDEXED_COLUMNS: &[&str] = &[
    "seq_num",
    "event_num",
    "timestamp",
    "tx_digest",
    "event_type",
    "package_id",
    "module_name",
    "sender",
    "recipient",
    "object_id",
    "object_type",
    "move_event_name",
];

impl SqlEventStore {
    /// Creates a new SQLite in-memory database, mostly for testing
    pub async fn new_memory_only_not_prod() -> Result<Self, SuiError> {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .map_err(convert_sqlx_err)?;
        info!("Created new in-memory SQLite EventStore for testing");
        Ok(Self { pool })
    }

    /// Creates or opens a new SQLite database at a specific path
    pub async fn new_from_file(db_path: &Path) -> Result<Self, SuiError> {
        // TODO: configure other SQLite options
        let mut options = SqliteConnectOptions::new()
            .filename(db_path)
            // SQLite turns off WAL by default and uses DELETE journaling.  WAL is at least 2x faster.
            .journal_mode(SqliteJournalMode::Wal)
            // Normal vs Full sync mode also speeds up writes
            .synchronous(SqliteSynchronous::Normal)
            // Minimal journal size and frequent autocheckpoints help prevent giant WALs
            .pragma("journal_size_limit", "0")
            .pragma("wal_autocheckpoint", "400") // In pages of 4KB each
            .create_if_missing(true);
        options.log_statements(log::LevelFilter::Off);
        let pool = SqlitePool::connect_with(options)
            .await
            .map_err(convert_sqlx_err)?;
        info!(?db_path, "Created/opened SQLite EventStore on disk");

        Ok(Self { pool })
    }

    /// Starts a WAL truncation/cleanup periodic task at interval duration
    pub async fn wal_cleanup_thread(&self, wal_cleanup_interval: Option<Duration>) {
        if let Some(cleanup_interval) = wal_cleanup_interval {
            let mut interval = tokio::time::interval(cleanup_interval);
            loop {
                interval.tick().await;
                info!("Running SQLite WAL truncation...");
                let _ = self.force_wal_truncation().await.map_err(|e| {
                    warn!("Unable to truncate Event Store SQLite WAL: {}", e);
                });
            }
        }
    }

    /// Force the SQLite WAL to be truncated.  This mighyt be occasionally necessary if somehow the WAL
    /// grows too big.
    #[instrument(level = "debug", skip_all, err)]
    pub async fn force_wal_truncation(&self) -> Result<(), SuiError> {
        self.pool
            .execute("PRAGMA wal_checkpoint(TRUNCATE)")
            .await
            .map_err(convert_sqlx_err)?;
        Ok(())
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

        self.pool
            .execute(
                "CREATE UNIQUE INDEX IF NOT EXISTS event_unique_id_idx on events (seq_num, event_num)",
            )
            .await
            .map_err(convert_sqlx_err)?;

        Ok(())
    }

    /// Returns total size of table.  Should really only be used for testing.
    #[allow(unused)]
    pub async fn total_event_count(&self) -> Result<usize, SuiError> {
        let result = sqlx::query("SELECT COUNT(*) FROM events")
            .fetch_one(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        let num_rows: i64 = result.get(0);
        Ok(num_rows as usize)
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

    // convert an event's extra fields into a stringified JSON Value.
    fn event_to_json(event: &EventEnvelope) -> String {
        // For move events, we only store the move_struct_json_value
        if let Some(json_value) = &event.move_struct_json_value {
            json_value.to_string()
        } else {
            // For non-move-events, extract whatever we can to rebuild the event
            // and store them
            let mut fields = BTreeMap::new();
            if let Some(object_version) = event.event.object_version().map(|ov| ov.value()) {
                fields.insert(OBJECT_VERSION_KEY, object_version.to_string());
            }
            if let Some(amount) = event.event.amount() {
                fields.insert(AMOUNT_KEY, amount.to_string());
            }
            if let Some(change_type) = event.event.balance_change_type() {
                fields.insert(BALANCE_CHANGE_TYPE_KEY, (*change_type as usize).to_string());
            }
            json!(fields).to_string()
        }
    }
}

impl From<SqliteRow> for StoredEvent {
    // Translate a Row into StoredEvent
    // TODO: convert to use FromRow trait so query_as() could be used?
    // TODO: gracefully handle data corruption/incompatibility without panicking
    fn from(row: SqliteRow) -> Self {
        let timestamp: i64 = row.get(EventsTableColumns::Timestamp as usize);
        let event_num: i64 = row.get(EventsTableColumns::EventNum as usize);
        let seq_num: i64 = row.get(EventsTableColumns::SeqNum as usize);
        let digest_raw: Option<Vec<u8>> = row.get(EventsTableColumns::TxDigest as usize);
        let tx_digest = digest_raw.map(|bytes| {
            TransactionDigest::new(
                bytes
                    .try_into()
                    .expect("Error converting digest bytes to TxDigest"),
            )
        });
        let event_type: u16 = row.get(EventsTableColumns::EventType as usize);
        let package_id =
            SqlEventStore::try_extract_object_id(&row, EventsTableColumns::PackageId as usize)
                .expect("Error converting stored package ID bytes to ObjectID");
        let object_id =
            SqlEventStore::try_extract_object_id(&row, EventsTableColumns::ObjectId as usize)
                .expect("Error converting stored object ID bytes to ObjectID");
        let object_type: Option<String> = row.get(EventsTableColumns::ObjectType as usize);
        let module_name: Option<String> = row.get(EventsTableColumns::ModuleName as usize);
        let function: Option<String> = row.get(EventsTableColumns::Function as usize);
        let fields_text: &str = row.get(EventsTableColumns::Fields as usize);
        let fields: BTreeMap<SharedStr, EventValue> = if fields_text.is_empty() {
            BTreeMap::new()
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
                BTreeMap::new()
            }
        };
        let move_event_contents: Option<Vec<u8>> = row.get(EventsTableColumns::Contents as usize);
        let move_event_name: Option<String> = row.get(EventsTableColumns::MoveEventName as usize);
        let sender = SqlEventStore::try_extract_sender_address(&row)
            .expect("Error converting stored sender address bytes to SuiAddress");
        let recipient = SqlEventStore::try_extract_recipient(&row)
            .expect("Error converting stored recipient address to Owner");

        StoredEvent {
            id: (seq_num, event_num).into(),
            timestamp: timestamp as u64,
            tx_digest,
            event_type: SharedStr::from(Event::name_from_ordinal(event_type as usize)),
            package_id,
            module_name: module_name.map(|s| s.into()),
            function_name: function.map(SharedStr::from),
            object_type,
            object_id,
            fields,
            move_event_contents,
            move_event_name,
            sender,
            recipient,
        }
    }
}
/// Maximum number of rows to insert at once as a batch.  SQLite has 64k limit in binding values.
const MAX_INSERT_BATCH: usize = 1000;

#[async_trait]
impl EventStore for SqlEventStore {
    #[instrument(level = "debug", skip_all, err)]
    async fn add_events(&self, events: &[EventEnvelope]) -> Result<u64, SuiError> {
        let mut rows_affected = 0;

        if events.is_empty() {
            return Ok(0);
        }

        for chunk in events.chunks(MAX_INSERT_BATCH) {
            let mut query_builder = QueryBuilder::new(SQL_INSERT_TX);
            query_builder.push_values(chunk, |mut b, event| {
                let event_type = EventType::from(&event.event);
                let sender = event.event.sender().map(|sender| sender.to_vec());
                let move_event_name = event.event.move_event_name();
                b.push_bind(event.timestamp as i64)
                    .push_bind(event.seq_num as i64)
                    .push_bind(event.event_num as i64)
                    .push_bind(event.tx_digest.map(|txd| txd.to_bytes()))
                    .push_bind(event_type as u16)
                    .push_bind(event.event.package_id().map(|pid| pid.to_vec()))
                    .push_bind(event.event.module_name())
                    .push_bind(event.event.object_id().map(|id| id.to_vec()))
                    .push_bind(event.event.object_type())
                    .push_bind(Self::event_to_json(event))
                    .push_bind(move_event_name)
                    .push_bind(event.event.move_event_contents())
                    .push_bind(sender)
                    .push_bind(
                        event
                            .event
                            .recipient_serialized()
                            .expect("Cannot serialize"),
                    );
            });

            let res = query_builder
                .build()
                .execute(&self.pool)
                .await
                .map_err(convert_sqlx_err)?;

            rows_affected += res.rows_affected();
        }

        Ok(rows_affected)
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn all_events(
        &self,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let query = get_event_query(vec![], descending);
        let rows = sqlx::query(&query)
            .persistent(true)
            .bind(cursor.tx_seq)
            .bind(cursor.event_seq)
            .bind(limit as i64)
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn events_by_transaction(
        &self,
        digest: TransactionDigest,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let query = get_event_query(vec![("tx_digest", Comparator::Equal)], descending);
        let rows = sqlx::query(&query)
            .persistent(true)
            .bind(cursor.tx_seq)
            .bind(cursor.event_seq)
            .bind(digest.to_bytes())
            .bind(limit as i64)
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn events_by_type(
        &self,
        event_type: EventType,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let query = get_event_query(vec![("event_type", Comparator::Equal)], descending);
        let rows = sqlx::query(&query)
            .persistent(true)
            .bind(cursor.tx_seq)
            .bind(cursor.event_seq)
            .bind(event_type as u16)
            .bind(limit as i64)
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn event_iterator(
        &self,
        start_time: u64,
        end_time: u64,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let query = get_event_query(
            vec![
                ("timestamp", Comparator::MoreThanOrEq),
                ("timestamp", Comparator::LessThan),
            ],
            descending,
        );
        let rows = sqlx::query(&query)
            .bind(cursor.tx_seq)
            .bind(cursor.event_seq)
            .bind(start_time as i64)
            .bind(end_time as i64)
            .bind(limit as i64)
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn events_by_module_id(
        &self,
        module: &ModuleId,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let query = get_event_query(
            vec![
                ("package_id", Comparator::Equal),
                ("module_name", Comparator::Equal),
            ],
            descending,
        );
        let rows = sqlx::query(&query)
            .persistent(true)
            .bind(cursor.tx_seq)
            .bind(cursor.event_seq)
            .bind(module.address().to_vec())
            .bind(module.name().to_string())
            .bind(limit as i64)
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }

    /// Possible to give part of the name in the query
    #[instrument(level = "debug", skip_all, err)]
    async fn events_by_move_event_struct_name(
        &self,
        move_event_struct_name: &str,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let query = get_event_query(vec![("move_event_name", Comparator::Like)], descending);
        let comparand = format!("{}%", move_event_struct_name);
        // TODO: duplication: these 10 lines are repetitive (4 times) in this file.
        let rows = sqlx::query(&query)
            .persistent(true)
            .bind(cursor.tx_seq)
            .bind(cursor.event_seq)
            .bind(comparand)
            .bind(limit as i64)
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn events_by_sender(
        &self,
        sender: &SuiAddress,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let query = get_event_query(vec![("sender", Comparator::Equal)], descending);
        let sender_vec = sender.to_vec();
        let rows = sqlx::query(&query)
            .persistent(true)
            .bind(cursor.tx_seq)
            .bind(cursor.event_seq)
            .bind(sender_vec)
            .bind(limit as i64)
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn events_by_recipient(
        &self,
        recipient: &Owner,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let query = get_event_query(vec![("recipient", Comparator::Equal)], descending);
        let recipient_str =
            serde_json::to_string(recipient).map_err(|e| SuiError::OwnerFailedToSerialize {
                error: (e.to_string()),
            })?;
        let rows = sqlx::query(&query)
            .persistent(true)
            .bind(cursor.tx_seq)
            .bind(cursor.event_seq)
            .bind(recipient_str)
            .bind(limit as i64)
            .map(StoredEvent::from)
            .fetch_all(&self.pool)
            .await
            .map_err(convert_sqlx_err)?;
        Ok(rows)
    }

    #[instrument(level = "debug", skip_all, err)]
    async fn events_by_object(
        &self,
        object: &ObjectID,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError> {
        let query = get_event_query(vec![("object_id", Comparator::Equal)], descending);
        let object_vec = object.to_vec();
        let rows = sqlx::query(&query)
            .persistent(true)
            .bind(cursor.tx_seq)
            .bind(cursor.event_seq)
            .bind(object_vec)
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

fn get_event_query(causes: Vec<(&str, Comparator)>, descending: bool) -> String {
    let (seq_cmp, order) = if descending {
        (Comparator::LessThanOrEq, "DESC")
    } else {
        (Comparator::MoreThanOrEq, "ASC")
    };
    let mut query =
        format!("SELECT * FROM events WHERE seq_num {seq_cmp} ? AND event_num {seq_cmp} ?");
    if !causes.is_empty() {
        query.push_str(" AND ");
    }
    let causes = causes
        .iter()
        .map(|(cause, cmp)| format!("{cause} {cmp} ?"))
        .collect::<Vec<_>>()
        .join(" AND ");
    query.push_str(&causes);
    query.push_str(&format!(
        " ORDER BY seq_num {order}, event_num {order} LIMIT ?"
    ));
    query
}

enum Comparator {
    Equal,
    LessThanOrEq,
    MoreThanOrEq,
    LessThan,
    Like,
}

impl Display for Comparator {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Comparator::Equal => "=",
            Comparator::LessThanOrEq => "<=",
            Comparator::MoreThanOrEq => ">=",
            Comparator::LessThan => "<",
            Comparator::Like => "LIKE",
        };
        write!(f, "{s}")
    }
}

#[cfg(test)]
mod tests {
    use flexstr::shared_str;
    use move_core_types::{account_address::AccountAddress, identifier::Identifier};

    use sui_types::event::EventEnvelope;

    use super::test_utils;
    use super::*;

    fn test_queried_event_vs_test_envelope(queried: &StoredEvent, orig: &EventEnvelope) {
        assert_eq!(queried.timestamp, orig.timestamp);
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
        assert_eq!(queried.object_type, orig.event.object_type());
        assert_eq!(
            queried.object_version().unwrap().as_ref(),
            orig.event.object_version()
        );
        assert_eq!(
            queried.move_event_contents.as_deref(),
            orig.event.move_event_contents()
        );
        assert_eq!(queried.amount().unwrap(), orig.event.amount());
        let move_event_name = orig.event.move_event_name();
        assert_eq!(queried.move_event_name.as_ref(), move_event_name.as_ref());
    }

    #[tokio::test]
    async fn test_eventstore_basic_insert_read() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        // Initialize store
        let db = SqlEventStore::new_memory_only_not_prod().await?;
        db.initialize().await?;

        // Insert some records
        info!("Inserting records!");
        let txfr_digest = TransactionDigest::random();
        let to_insert = vec![
            test_utils::new_test_newobj_event(
                1_000_000,
                TransactionDigest::random(),
                1,
                0, // event_num
                None,
                None,
                None,
            ),
            test_utils::new_test_publish_event(
                1_001_000,
                TransactionDigest::random(),
                2,
                0, // event_num
                None,
            ),
            test_utils::new_test_transfer_event(
                1_002_000,
                txfr_digest,
                3,
                0, // event_num
                1,
                "0x2::test::Object",
                None,
                None,
                None,
            ),
            test_utils::new_test_deleteobj_event(
                1_003_000,
                txfr_digest,
                3,
                1, // event_num
                None,
                None,
            ),
            test_utils::new_test_transfer_event(
                1_004_000,
                TransactionDigest::random(),
                4,
                0, // event_num
                1,
                "0x2::test::Object",
                None,
                None,
                None,
            ),
            test_utils::new_test_move_event(
                1_005_000,
                TransactionDigest::random(),
                5,
                0, // event_num
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
                "test_foo",
            ),
            test_utils::new_test_balance_change_event(1_006_000, 6, 0, None, None, None),
            test_utils::new_test_mutate_event(1_007_000, 7, 0, 1, "0x2::test::Object", None, None),
        ];
        assert_eq!(db.add_events(&to_insert).await?, 8);
        info!("Done inserting");

        assert_eq!(db.total_event_count().await?, 8);

        // Query for records in time range, end should be exclusive - should get 8
        let queried_events = db
            .event_iterator(1_000_000, 1_008_000, (0, 0).into(), 20, false)
            .await?;
        assert_eq!(queried_events.len(), 8);
        for i in 0..8 {
            // ASCENDING order
            test_queried_event_vs_test_envelope(&queried_events[i], &to_insert[i]);
        }

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
            test_utils::new_test_newobj_event(
                1_000_000,
                TransactionDigest::random(),
                1,
                0, // event_num
                None,
                None,
                None,
            ),
            test_utils::new_test_publish_event(
                1_001_000,
                TransactionDigest::random(),
                2,
                0, // event_num
                None,
            ),
            test_utils::new_test_transfer_event(
                1_003_000,
                TransactionDigest::random(),
                3,
                0, // event_num
                1,
                "0x2::test:Object",
                None,
                None,
                None,
            ),
            test_utils::new_test_deleteobj_event(
                1_003_000,
                TransactionDigest::random(),
                4,
                0, // event_num
                None,
                None,
            ),
            test_utils::new_test_transfer_event(
                1_004_000,
                TransactionDigest::random(),
                5,
                0, // event_num
                1,
                "0x2::test:Object",
                None,
                None,
                None,
            ),
            test_utils::new_test_move_event(
                1_005_000,
                TransactionDigest::random(),
                6,
                0, // event_num
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
                "test_foo",
            ),
        ];
        db.add_events(&to_insert).await?;
        let target_event = &to_insert[2];
        info!("Done inserting");

        // Query for transfer event
        let mut events = db
            .events_by_transaction(target_event.tx_digest.unwrap(), (0, 0).into(), 10, false)
            .await?;
        assert_eq!(events.len(), 1); // Should be no more events, just that one
        let transfer_event = events.pop().unwrap();

        test_queried_event_vs_test_envelope(&transfer_event, target_event);

        assert_eq!(transfer_event.fields.len(), 1); // obj ver

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
        let txfr_digest = TransactionDigest::random();
        let to_insert = vec![
            test_utils::new_test_newobj_event(
                1_000_000,
                TransactionDigest::random(),
                1,
                0, // event_num
                None,
                None,
                None,
            ),
            test_utils::new_test_publish_event(
                1_001_000,
                TransactionDigest::random(),
                2,
                1, // event_num
                None,
            ),
            test_utils::new_test_transfer_event(
                1_003_000,
                txfr_digest,
                3,
                0, // event_num
                1,
                "0x2::test:Object",
                None,
                None,
                None,
            ),
            test_utils::new_test_deleteobj_event(
                1_003_000,
                txfr_digest,
                3,
                1, // event_num
                None,
                None,
            ),
            test_utils::new_test_transfer_event(
                1_004_000,
                TransactionDigest::random(),
                4,
                0, // event_num
                1,
                "0x2::test:Object",
                None,
                None,
                None,
            ),
            test_utils::new_test_move_event(
                1_005_000,
                TransactionDigest::random(),
                5,
                0, // event_num
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
                "test_foo",
            ),
            test_utils::new_test_balance_change_event(1_006_000, 6, 0, None, None, None),
        ];
        db.add_events(&to_insert).await?;
        info!("Done inserting");

        let queried_events = db
            .events_by_type(EventType::TransferObject, (3, 0).into(), 2, false)
            .await?;
        assert_eq!(queried_events.len(), 2);

        // Desc timestamp order, so the last transfer event should be first
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[2]);
        test_queried_event_vs_test_envelope(&queried_events[1], &to_insert[4]);

        // Query again with limit of 1, it should return only the last transfer event
        let queried_events = db
            .events_by_type(EventType::TransferObject, (3, 0).into(), 1, false)
            .await?;
        assert_eq!(queried_events.len(), 1);
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[2]);
        assert_eq!(queried_events[0].fields.len(), 1);

        // Query with wrong time range, return 0 events
        let queried_events = db
            .events_by_type(EventType::TransferObject, (6, 0).into(), 1, false)
            .await?;
        assert_eq!(queried_events.len(), 0);

        // Query Publish Event
        let queried_events = db
            .events_by_type(EventType::Publish, (2, 0).into(), 1, false)
            .await?;
        assert_eq!(queried_events.len(), 1);
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[1]);
        assert_eq!(queried_events[0].fields.len(), 0);

        // Query NewObject Event
        let queried_events = db
            .events_by_type(EventType::NewObject, (0, 0).into(), 1, false)
            .await?;
        assert_eq!(queried_events.len(), 1);
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[0]);
        assert_eq!(queried_events[0].fields.len(), 1); // version field

        // Query DeleteObject Event
        let queried_events = db
            .events_by_type(EventType::DeleteObject, (3, 0).into(), 1, false)
            .await?;
        assert_eq!(queried_events.len(), 1);
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[3]);
        assert_eq!(queried_events[0].fields.len(), 1); // version

        // Query Move Event
        let queried_events = db
            .events_by_type(EventType::MoveEvent, (4, 0).into(), 1, false)
            .await?;
        assert_eq!(queried_events.len(), 1);
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[5]);
        assert_ne!(queried_events[0].fields.len(), 0);

        // Query Balance Change Event
        let queried_events = db
            .events_by_type(EventType::CoinBalanceChange, (6, 0).into(), 1, false)
            .await?;
        assert_eq!(queried_events.len(), 1);
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[6]);
        assert_eq!(queried_events[0].fields.len(), 3); // amount, version, balance change type.
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
            test_utils::new_test_newobj_event(
                1_000_000,
                TransactionDigest::random(),
                1,
                0, // event_num
                None,
                None,
                None,
            ),
            test_utils::new_test_publish_event(
                1_001_000,
                TransactionDigest::random(),
                2,
                0, // event_num
                None,
            ),
            test_utils::new_test_transfer_event(
                1_002_000,
                TransactionDigest::random(),
                3,
                0, // event_num
                1,
                "0x2::test:Object",
                None,
                None,
                None,
            ),
            test_utils::new_test_deleteobj_event(
                1_003_000,
                TransactionDigest::random(),
                3,
                0, // event_num
                None,
                None,
            ),
            test_utils::new_test_transfer_event(
                1_004_000,
                TransactionDigest::random(),
                4,
                0, // event_num
                1,
                "0x2::test:Object",
                None,
                None,
                None,
            ),
            test_utils::new_test_move_event(
                1_005_000,
                TransactionDigest::random(),
                5,
                0, // event_num
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
                "test_foo",
            ),
            test_utils::new_test_move_event(
                1_006_000,
                TransactionDigest::random(),
                6,
                0, // event_num
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
                "test_foo",
            ),
        ];
        db.add_events(&to_insert).await?;
        info!("Done inserting");

        // Query for the Move event and validate basic fields
        let events = db
            .events_by_transaction(to_insert[5].tx_digest.unwrap(), (0, 0).into(), 10, false)
            .await?;
        let move_event = &events[0];
        assert_eq!(events.len(), 1); // Should be no more events, just that one

        test_queried_event_vs_test_envelope(move_event, &to_insert[5]);
        assert_eq!(move_event.fields.len(), 2);

        // Query by module ID
        let mod_id = ModuleId::new(
            AccountAddress::from(ObjectID::from_hex_literal("0x3").unwrap()),
            Identifier::from_str("test_module").unwrap(),
        );
        let queried_events = db
            .events_by_module_id(&mod_id, (0, 0).into(), 3, false)
            .await?;
        assert_eq!(queried_events.len(), 2);

        // results are sorted in DESC order
        test_queried_event_vs_test_envelope(&queried_events[0], &to_insert[5]);
        test_queried_event_vs_test_envelope(&queried_events[1], &to_insert[6]);
        assert_eq!(queried_events[0].fields.len(), 2);
        assert_eq!(queried_events[1].fields.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_eventstore_query_by_move_event_struct_name() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        // Initialize store
        let db = SqlEventStore::new_memory_only_not_prod().await?;
        db.initialize().await?;

        // Insert some records
        info!("Inserting records!");
        let to_insert = vec![
            test_utils::new_test_move_event(
                1_000_000,
                TransactionDigest::random(),
                1,
                0, // event_num
                ObjectID::from_hex_literal("0x42").unwrap(),
                "query_by_move_event_struct_name",
                "test_foo",
            ),
            test_utils::new_test_move_event(
                1_001_000,
                TransactionDigest::random(),
                2,
                0, // event_num
                ObjectID::from_hex_literal("0x42").unwrap(),
                "query_by_move_event_struct_name",
                "test_foo",
            ),
            test_utils::new_test_move_event(
                1_002_000,
                TransactionDigest::random(),
                3,
                0, // event_num
                ObjectID::from_hex_literal("0x42").unwrap(),
                "query_by_move_event_struct_name",
                "test_bar",
            ),
        ];

        assert_eq!(db.add_events(&to_insert).await?, 3);
        info!("Done inserting");

        let events = db
            .events_by_move_event_struct_name(
                "0x2::SUI::test_foo<address, vector<u8>>",
                (0, 0).into(),
                10,
                false,
            )
            .await?;
        assert_eq!(events.len(), 2);

        test_queried_event_vs_test_envelope(&events[0], &to_insert[0]);
        test_queried_event_vs_test_envelope(&events[1], &to_insert[1]);
        assert_eq!(events[0].fields.len(), 2);
        assert_eq!(events[1].fields.len(), 2);

        // Test querying by only part of the name, or just package and module (prefix search)
        let events = db
            .events_by_move_event_struct_name("0x2::SUI::", (0, 0).into(), 10, false)
            .await?;
        assert_eq!(events.len(), 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_eventstore_query_by_sender_recipient_and_object() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        // Initialize store
        let db = SqlEventStore::new_memory_only_not_prod().await?;
        db.initialize().await?;

        // Insert some records
        info!("Inserting records!");
        let sender = SuiAddress::random_for_testing_only();
        let recipient = Owner::AddressOwner(SuiAddress::random_for_testing_only());
        let object_id = ObjectID::random();
        let to_insert = vec![
            test_utils::new_test_transfer_event(
                // 0, object, sender, recipient
                1_000_000,
                TransactionDigest::random(),
                1,
                0, // event_num
                1,
                "0x2::test:Object",
                Some(object_id),
                Some(sender),
                Some(recipient),
            ),
            test_utils::new_test_newobj_event(
                // 1, object, sender
                1_001_000,
                TransactionDigest::random(),
                2,
                0, // event_num
                Some(object_id),
                Some(sender),
                None,
            ),
            test_utils::new_test_transfer_event(
                // 2, recipient
                1_002_000,
                TransactionDigest::random(),
                3,
                0, // event_num
                1,
                "0x2::test:Object",
                None,
                None,
                Some(recipient),
            ),
            test_utils::new_test_newobj_event(
                // 3, object, recipient
                1_003_000,
                TransactionDigest::random(),
                4,
                0, // event_num
                Some(object_id),
                None,
                Some(recipient),
            ),
            test_utils::new_test_deleteobj_event(
                // 4, object, sender
                1_004_000,
                TransactionDigest::random(),
                5,
                0, // event_num
                Some(object_id),
                Some(sender),
            ),
            test_utils::new_test_deleteobj_event(
                // 5, sender
                1_005_000,
                TransactionDigest::random(),
                6,
                0, // event_num
                None,
                Some(sender),
            ),
            test_utils::new_test_publish_event(
                // 6, None
                1_006_000,
                TransactionDigest::random(),
                7,
                0, // event_num
                None,
            ),
            test_utils::new_test_publish_event(
                // 7, sender
                1_007_000,
                TransactionDigest::random(),
                8,
                0, // event_num
                Some(sender),
            ),
        ];

        assert_eq!(db.add_events(&to_insert).await?, 8);
        info!("Done inserting");

        // Query by sender
        let events = db
            .events_by_sender(&sender, (0, 0).into(), 10, false)
            .await?;
        assert_eq!(events.len(), 5);

        test_queried_event_vs_test_envelope(&events[0], &to_insert[0]);
        test_queried_event_vs_test_envelope(&events[1], &to_insert[1]);
        test_queried_event_vs_test_envelope(&events[2], &to_insert[4]);
        test_queried_event_vs_test_envelope(&events[3], &to_insert[5]);
        test_queried_event_vs_test_envelope(&events[4], &to_insert[7]);

        // Query by recipient
        let events = db
            .events_by_recipient(&recipient, (0, 0).into(), 10, false)
            .await?;
        assert_eq!(events.len(), 3);

        test_queried_event_vs_test_envelope(&events[0], &to_insert[0]);
        test_queried_event_vs_test_envelope(&events[1], &to_insert[2]);
        test_queried_event_vs_test_envelope(&events[2], &to_insert[3]);

        // Query by object
        let events = db
            .events_by_object(&object_id, (0, 0).into(), 10, false)
            .await?;
        assert_eq!(events.len(), 4);

        test_queried_event_vs_test_envelope(&events[0], &to_insert[0]);
        test_queried_event_vs_test_envelope(&events[1], &to_insert[1]);
        test_queried_event_vs_test_envelope(&events[2], &to_insert[3]);
        test_queried_event_vs_test_envelope(&events[3], &to_insert[4]);

        Ok(())
    }

    // Test we can retrieve u64 object version (aka sequence number) values
    // stored as string in sqlite
    #[tokio::test]
    async fn test_eventstore_u64_conversion() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        let db = SqlEventStore::new_memory_only_not_prod().await?;
        db.initialize().await?;

        let to_insert = vec![test_utils::new_test_transfer_event(
            1_000_000,
            TransactionDigest::random(),
            1,
            0, // event_num
            u64::MAX,
            "0x2::test:Object",
            None,
            None,
            None,
        )];
        db.add_events(&to_insert).await?;

        let events = db
            .events_by_transaction(to_insert[0].tx_digest.unwrap(), (0, 0).into(), 10, false)
            .await?;
        assert_eq!(events.len(), 1);
        info!("events[0]: {:?}", events[0]);
        assert_eq!(
            events[0].object_version().unwrap().unwrap().value(),
            u64::MAX
        );
        Ok(())
    }

    // Test Idempotency / Sequence Numbering
    #[tokio::test]
    async fn test_eventstore_seq_num() -> Result<(), SuiError> {
        telemetry_subscribers::init_for_testing();

        // Initialize store
        let dir = tempfile::TempDir::new().unwrap(); // NOTE this must be its own line so dir isn't dropped
        let db_file = dir.path().join("events.db");
        let db = SqlEventStore::new_from_file(&db_file).await.unwrap();
        db.initialize().await.unwrap();

        let txfr_digest = TransactionDigest::random();
        // TODO: these 30 lines are quite duplicated in this file (4 times).
        // Write in some events, all should succeed
        let to_insert = vec![
            test_utils::new_test_newobj_event(
                1_000_000,
                TransactionDigest::random(),
                1,
                0, // event_num
                None,
                None,
                None,
            ),
            test_utils::new_test_publish_event(
                1_001_000,
                TransactionDigest::random(),
                2,
                0, // event_num
                None,
            ),
            test_utils::new_test_transfer_event(
                1_002_000,
                txfr_digest,
                3,
                0, // event_num
                1,
                "0x2::test:Object",
                None,
                None,
                None,
            ),
            test_utils::new_test_deleteobj_event(
                1_003_000,
                txfr_digest,
                3,
                1, // event_num
                None,
                None,
            ),
            test_utils::new_test_transfer_event(
                1_004_000,
                TransactionDigest::random(),
                4,
                0, // event_num
                1,
                "0x2::test:Object",
                None,
                None,
                None,
            ),
            test_utils::new_test_move_event(
                1_005_000,
                TransactionDigest::random(),
                5,
                0, // event_num
                ObjectID::from_hex_literal("0x3").unwrap(),
                "test_module",
                "test_foo",
            ),
        ];
        assert_eq!(db.add_events(&to_insert[..4]).await.unwrap(), 4);
        assert_eq!(db.total_event_count().await.unwrap(), 4);

        // Previously inserted event is ignored
        assert_eq!(db.add_events(&to_insert[1..2]).await.unwrap(), 0);
        assert_eq!(db.total_event_count().await.unwrap(), 4);

        // Drop and reload DB from the same file.
        drop(db);
        let db = SqlEventStore::new_from_file(&db_file).await.unwrap();
        db.initialize().await.unwrap();
        assert_eq!(db.total_event_count().await.unwrap(), 4);

        // Try inserting previously ingested events, should be skipped
        assert_eq!(db.add_events(&to_insert[1..2]).await.unwrap(), 0);
        assert_eq!(db.total_event_count().await.unwrap(), 4);

        // Check writing new events still succeeds
        assert_eq!(db.add_events(&to_insert[4..]).await.unwrap(), 2);
        assert_eq!(db.total_event_count().await.unwrap(), 6);

        Ok(())
    }

    #[test]
    fn event_query_test() {
        let query = get_event_query(vec![], false);
        assert_eq!(
            "SELECT * FROM events WHERE seq_num >= ? AND event_num >= ? ORDER BY seq_num ASC, event_num ASC LIMIT ?",
            query
        );
        let query = get_event_query(vec![], true);
        assert_eq!(
            "SELECT * FROM events WHERE seq_num <= ? AND event_num <= ? ORDER BY seq_num DESC, event_num DESC LIMIT ?",
            query
        );

        let query = get_event_query(vec![("event_type", Comparator::Equal)], false);
        assert_eq!("SELECT * FROM events WHERE seq_num >= ? AND event_num >= ? AND event_type = ? ORDER BY seq_num ASC, event_num ASC LIMIT ?", query);

        let query = get_event_query(vec![("event_type", Comparator::Equal)], true);
        assert_eq!("SELECT * FROM events WHERE seq_num <= ? AND event_num <= ? AND event_type = ? ORDER BY seq_num DESC, event_num DESC LIMIT ?", query);

        let query = get_event_query(vec![("event_type", Comparator::Equal)], true);
        assert_eq!("SELECT * FROM events WHERE seq_num <= ? AND event_num <= ? AND event_type = ? ORDER BY seq_num DESC, event_num DESC LIMIT ?", query);

        let query = get_event_query(
            vec![
                ("package_id", Comparator::Equal),
                ("module_name", Comparator::Equal),
            ],
            false,
        );
        assert_eq!("SELECT * FROM events WHERE seq_num >= ? AND event_num >= ? AND package_id = ? AND module_name = ? ORDER BY seq_num ASC, event_num ASC LIMIT ?", query);

        let query = get_event_query(
            vec![
                ("package_id", Comparator::Equal),
                ("module_name", Comparator::Equal),
            ],
            true,
        );
        assert_eq!("SELECT * FROM events WHERE seq_num <= ? AND event_num <= ? AND package_id = ? AND module_name = ? ORDER BY seq_num DESC, event_num DESC LIMIT ?", query);
    }
}
