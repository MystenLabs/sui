//! SQL and SQLite-based Event Store

use super::*;

use async_trait::async_trait;

use sqlx::{Connection, Executor, SqlitePool};
use tracing::info;

pub struct SqlEventStore {
    pool: SqlitePool,
}

const SQL_TABLE_CREATE: &str = "\
    CREATE TABLE IF NOT EXISTS events(
        timestamp INTEGER NOT NULL,
        checkpoint INTEGER,
        tx_digest BLOB,
        event_type TEXT,
        package_id BLOB,
        module_name TEXT,
        object_id BLOB,
        fields TEXT,
    );
";

const SQL_INDEX_CREATE: &str = "CREATE INDEX IF NOT EXISTS ? ON events(?)";

const INDEXED_COLUMNS: &[&str] = &[
    "timestamp",
    "tx_digest",
    "event_type",
    "package_id",
    "module_name",
];

impl SqlEventStore {
    /// Creates a new SQLite database for event storage
    // TODO: add parameter for connection string
    pub async fn new_sqlite() -> Result<Self, EventStoreError> {
        let pool = SqlitePool::connect("sqlite::memory:").await?;
        info!("Created new SQLite EventStore");
        Ok(Self { pool })
    }

    async fn create_index(&self, column: &str) -> Result<(), EventStoreError> {
        sqlx::query(SQL_INDEX_CREATE)
            .bind(format!("{}_index", column))
            .bind(column)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Initializes the database, creating tables and indexes as needed
    /// It should be safe to call this every time after new_sqlite() as IF NOT EXISTS are used.
    pub async fn initialize(&self) -> Result<(), EventStoreError> {
        // First create the table if needed
        self.pool.execute(SQL_TABLE_CREATE).await?;
        info!("SQLite events table created");

        // Then, create indexes
        for column in INDEXED_COLUMNS {
            self.create_index(*column).await?;
        }
        info!("Indexes created");

        Ok(())
    }
}

fn try_extract_object_id<R>(row: R, index: usize) -> Result<Option<ObjectID>, EventStoreError>
where
    R: sqlx::Row<Database = sqlx::Sqlite>,
{
    let raw_bytes: Option<Vec<u8>> = row.get(index.into());
    match raw_bytes {
        Some(bytes) => Ok(Some(
            ObjectID::try_from(bytes).map_err(|e| EventStoreError::GenericError(e.into()))?,
        )),
        None => Ok(None),
    }
}

// Translate a Row into StoredEvent
// TODO: convert to use FromRow trait so query_as() could be used?
fn sql_row_to_event<R>(row: R) -> StoredEvent
where
    R: sqlx::Row<Database = sqlx::Sqlite>,
{
    let timestamp: i64 = row.get(0.into());
    let checkpoint: i64 = row.get(1.into());
    let digest_raw: Option<Vec<u8>> = row.get(2.into());
    let tx_digest = digest_raw.map(|bytes| {
        TransactionDigest::new(
            bytes
                .try_into()
                .expect("Cannot convert digest bytes to TxDigest"),
        )
    });
    let event_type: String = row.get(3.into());
    let package_id = try_extract_object_id(row, 4).expect("Error converting package ID bytes");
    let module_name: Option<String> = row.get(5.into());

    StoredEvent {
        timestamp: timestamp as u64,
        checkpoint_num: checkpoint as u64,
        tx_digest,
        event_type: event_type.into(),
        package_id,
        module_name: module_name.map(|s| s.into()),
        fields: Vec::new(),
    }
}

const SQL_INSERT_TX: &str = "INSERT INTO events (timestamp, checkpoint, tx_digest, event_type, \
    package_id, module_name, object_id) VALUES (?, ?, ?, ?, ?, ?, ?)";

const TS_QUERY: &str = "SELECT * FROM events WHERE timestamp >= ? AND timestamp < ? LIMIT ?";

#[async_trait]
impl EventStore for SqlEventStore {
    // OK, the static is a lie, for now.
    type EventIt = std::slice::Iter<'static, StoredEvent>;

    async fn add_events(
        &self,
        events: &[EventEnvelope],
        checkpoint_num: u64,
    ) -> Result<(), EventStoreError> {
        let insert_tx_q = sqlx::query(SQL_INSERT_TX).persistent(true);
        for event in events {
            let module_id = event.event.module_id();
            // TODO: use batched API?
            insert_tx_q
                .bind(event.timestamp as i64)
                .bind(checkpoint_num as i64)
                .bind(event.tx_digest.map(|txd| txd.as_ref()))
                .bind(event.event_type())
                .bind(module_id.map(|mid| mid.address().as_ref()))
                .bind(module_id.map(|mid| mid.name().to_string()))
                .bind(event.event.object_id().map(|id| id.as_ref()))
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn event_iterator(
        &self,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<Self::EventIt, EventStoreError> {
        // TODO: check limit is not too high
        let rows = sqlx::query(TS_QUERY)
            .bind(start_time as i64)
            .bind(end_time as i64)
            .bind(limit as i64)
            .map(sql_row_to_event)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter())
    }
}
