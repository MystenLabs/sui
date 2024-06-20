// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::ProgressStore as DBProgressStore;
use crate::models::TokenTransfer as DBTokenTransfer;
use crate::models::TokenTransferData as DBTokenTransferData;
use crate::schema::progress_store::checkpoint;
use crate::schema::progress_store::dsl::progress_store;
use crate::schema::token_transfer_data;
use crate::{schema, schema::token_transfer, TokenTransfer};
use async_trait::async_trait;
use diesel::result::Error;
use diesel::BoolExpressionMethods;
use diesel::{
    pg::PgConnection,
    r2d2::{ConnectionManager, Pool},
    Connection, ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SelectableHelper,
};
use sui_data_ingestion_core::ProgressStore;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub(crate) type PgPool = Pool<ConnectionManager<PgConnection>>;

pub fn get_connection_pool(database_url: String) -> PgPool {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::builder()
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build Postgres DB connection pool")
}

// TODO: add retry logic
pub fn write(pool: &PgPool, token_txns: Vec<TokenTransfer>) -> Result<(), anyhow::Error> {
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

pub struct PgProgressStore {
    pool: PgPool,
    bridge_genesis_checkpoint: u64,
}

impl PgProgressStore {
    pub fn new(pool: PgPool, bridge_genesis_checkpoint: u64) -> Self {
        PgProgressStore {
            pool,
            bridge_genesis_checkpoint,
        }
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
            })
            .on_conflict(schema::progress_store::dsl::task_name)
            .do_update()
            .set(checkpoint.eq(checkpoint_number as i64))
            .execute(&mut conn)?;
        Ok(())
    }
}
