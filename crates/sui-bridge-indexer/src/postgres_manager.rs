// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::ProgressStore as DBProgressStore;
use crate::models::SuiProgressStore;
use crate::models::TokenTransfer as DBTokenTransfer;
use crate::models::TokenTransferData as DBTokenTransferData;
use crate::schema::progress_store::columns;
use crate::schema::progress_store::dsl::progress_store;
use crate::schema::sui_progress_store::txn_digest;
use crate::schema::token_transfer_data;
use crate::{schema, schema::token_transfer, TokenTransfer};
use async_trait::async_trait;
use diesel::dsl::now;
use diesel::result::Error;
use diesel::{delete, BoolExpressionMethods};
use diesel::{
    pg::PgConnection,
    r2d2::{ConnectionManager, Pool},
    Connection, ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SelectableHelper,
};
use sui_data_ingestion_core::ProgressStore;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub(crate) type PgPool = Pool<ConnectionManager<PgConnection>>;

const SUI_PROGRESS_STORE_DUMMY_KEY: i32 = 1;

pub fn get_connection_pool(database_url: String) -> PgPool {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::builder()
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build Postgres DB connection pool")
}

// TODO: add retry logic
pub fn write(pool: &PgPool, token_txns: Vec<TokenTransfer>) -> Result<(), anyhow::Error> {
    if token_txns.is_empty() {
        return Ok(());
    }
    let (transfers, data): (Vec<DBTokenTransfer>, Vec<Option<DBTokenTransferData>>) = token_txns
        .iter()
        .map(|t| (t.to_db(), t.to_data_maybe()))
        .unzip();

    let data = data.into_iter().flatten().collect::<Vec<_>>();

    let connection = &mut pool.get()?;
    connection.transaction(|conn| {
        diesel::insert_into(token_transfer_data::table)
            .values(&data)
            .on_conflict_do_nothing()
            .execute(conn)?;
        diesel::insert_into(token_transfer::table)
            .values(&transfers)
            .on_conflict_do_nothing()
            .execute(conn)
    })?;
    Ok(())
}

pub fn update_sui_progress_store(
    pool: &PgPool,
    tx_digest: TransactionDigest,
) -> Result<(), anyhow::Error> {
    let mut conn = pool.get()?;
    diesel::insert_into(schema::sui_progress_store::table)
        .values(&SuiProgressStore {
            id: SUI_PROGRESS_STORE_DUMMY_KEY,
            txn_digest: tx_digest.inner().to_vec(),
        })
        .on_conflict(schema::sui_progress_store::dsl::id)
        .do_update()
        .set(txn_digest.eq(tx_digest.inner().to_vec()))
        .execute(&mut conn)?;
    Ok(())
}

pub fn read_sui_progress_store(pool: &PgPool) -> anyhow::Result<Option<TransactionDigest>> {
    let mut conn = pool.get()?;
    let val: Option<SuiProgressStore> = crate::schema::sui_progress_store::dsl::sui_progress_store
        .select(SuiProgressStore::as_select())
        .first(&mut conn)
        .optional()?;
    match val {
        Some(val) => Ok(Some(TransactionDigest::try_from(
            val.txn_digest.as_slice(),
        )?)),
        None => Ok(None),
    }
}

pub fn get_latest_eth_token_transfer(
    pool: &PgPool,
    finalized: bool,
) -> Result<Option<DBTokenTransfer>, Error> {
    use crate::schema::token_transfer::dsl::*;

    let connection = &mut pool.get().unwrap();

    if finalized {
        token_transfer
            .filter(data_source.eq("ETH").and(status.eq("Deposited")))
            .order(block_height.desc())
            .first::<DBTokenTransfer>(connection)
            .optional()
    } else {
        token_transfer
            .filter(status.eq("DepositedUnfinalized"))
            .order(block_height.desc())
            .first::<DBTokenTransfer>(connection)
            .optional()
    }
}

#[derive(Clone)]
pub struct PgProgressStore {
    pool: PgPool,
    bridge_genesis_checkpoint: u64,
}

#[derive(Clone)]
pub struct Task {
    pub task_name: String,
    pub checkpoint: u64,
    pub target_checkpoint: u64,
    pub timestamp: u64,
}

impl From<DBProgressStore> for Task {
    fn from(value: DBProgressStore) -> Self {
        Self {
            task_name: value.task_name,
            checkpoint: value.checkpoint as u64,
            target_checkpoint: value.target_checkpoint as u64,
            // Ok to unwrap, timestamp is defaulted to now() in database
            timestamp: value.timestamp.expect("Timestamp not set").0 as u64,
        }
    }
}

impl PgProgressStore {
    pub fn new(pool: PgPool, bridge_genesis_checkpoint: u64) -> Self {
        // read all task from db
        PgProgressStore {
            pool,
            bridge_genesis_checkpoint,
        }
    }

    pub fn tasks(&self) -> Result<Vec<Task>, anyhow::Error> {
        let mut conn = self.pool.get()?;
        // clean up completed task
        delete(progress_store.filter(columns::checkpoint.ge(columns::target_checkpoint)))
            .execute(&mut conn)?;
        // get all unfinished tasks
        let cp: Vec<DBProgressStore> = progress_store
            .order_by(columns::checkpoint.desc())
            .load(&mut conn)?;
        Ok(cp.into_iter().map(|d| d.into()).collect())
    }

    pub fn register_task(
        &self,
        task_name: String,
        checkpoint: u64,
        target_checkpoint: i64,
    ) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get()?;
        diesel::insert_into(schema::progress_store::table)
            .values(DBProgressStore {
                task_name,
                checkpoint: checkpoint as i64,
                target_checkpoint,
                timestamp: None,
            })
            .execute(&mut conn)?;
        Ok(())
    }

    pub fn update_task(&self, task: Task) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get()?;
        diesel::update(progress_store.filter(columns::task_name.eq(task.task_name)))
            .set((
                columns::checkpoint.eq(task.checkpoint as i64),
                columns::target_checkpoint.eq(task.target_checkpoint as i64),
                columns::timestamp.eq(now),
            ))
            .execute(&mut conn)?;
        Ok(())
    }
}

pub trait Tasks {
    fn latest_checkpoint_task(&self) -> Option<Task>;
}

impl Tasks for Vec<Task> {
    fn latest_checkpoint_task(&self) -> Option<Task> {
        self.iter().fold(None, |result, other_task| match &result {
            Some(task) if task.checkpoint < other_task.checkpoint => Some(other_task.clone()),
            None => Some(other_task.clone()),
            _ => result,
        })
    }
}

#[async_trait]
impl ProgressStore for PgProgressStore {
    async fn load(&mut self, task_name: String) -> anyhow::Result<CheckpointSequenceNumber> {
        let mut conn = self.pool.get()?;
        let cp: Option<DBProgressStore> = progress_store
            .find(task_name)
            .select(DBProgressStore::as_select())
            .first(&mut conn)
            .optional()?;
        Ok(cp
            .map(|d| d.checkpoint as u64)
            .unwrap_or(self.bridge_genesis_checkpoint))
    }

    async fn save(
        &mut self,
        task_name: String,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> anyhow::Result<()> {
        let mut conn = self.pool.get()?;
        diesel::insert_into(schema::progress_store::table)
            .values(&DBProgressStore {
                task_name,
                checkpoint: checkpoint_number as i64,
                target_checkpoint: i64::MAX,
                timestamp: None,
            })
            .on_conflict(schema::progress_store::dsl::task_name)
            .do_update()
            .set((
                columns::checkpoint.eq(checkpoint_number as i64),
                columns::timestamp.eq(now),
            ))
            .execute(&mut conn)?;
        Ok(())
    }
}
