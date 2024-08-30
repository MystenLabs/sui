// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::SuiProgressStore;
use crate::models::TokenTransfer as DBTokenTransfer;
use crate::schema::sui_progress_store::txn_digest;
use crate::schema::{sui_error_transactions, token_transfer_data};
use crate::{schema, schema::token_transfer, ProcessedTxnData};
use diesel::result::Error;
use diesel::BoolExpressionMethods;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, SelectableHelper};
use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncConnection;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;
use sui_types::digests::TransactionDigest;

pub(crate) type PgPool =
    diesel_async::pooled_connection::bb8::Pool<diesel_async::AsyncPgConnection>;

const SUI_PROGRESS_STORE_DUMMY_KEY: i32 = 1;

pub async fn get_connection_pool(database_url: String) -> PgPool {
    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);

    Pool::builder()
        .test_on_check_out(true)
        .build(manager)
        .await
        .expect("Could not build Postgres DB connection pool")
}

// TODO: add retry logic
pub async fn write(pool: &PgPool, token_txns: Vec<ProcessedTxnData>) -> Result<(), anyhow::Error> {
    if token_txns.is_empty() {
        return Ok(());
    }
    let (transfers, data, errors) = token_txns.iter().fold(
        (vec![], vec![], vec![]),
        |(mut transfers, mut data, mut errors), d| {
            match d {
                ProcessedTxnData::TokenTransfer(t) => {
                    transfers.push(t.to_db());
                    if let Some(d) = t.to_data_maybe() {
                        data.push(d)
                    }
                }
                ProcessedTxnData::Error(e) => errors.push(e.to_db()),
            }
            (transfers, data, errors)
        },
    );

    let connection = &mut pool.get().await?;
    connection
        .transaction(|conn| {
            async move {
                diesel::insert_into(token_transfer_data::table)
                    .values(&data)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                diesel::insert_into(token_transfer::table)
                    .values(&transfers)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                diesel::insert_into(sui_error_transactions::table)
                    .values(&errors)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await
            }
            .scope_boxed()
        })
        .await?;
    Ok(())
}

pub async fn update_sui_progress_store(
    pool: &PgPool,
    tx_digest: TransactionDigest,
) -> Result<(), anyhow::Error> {
    let mut conn = pool.get().await?;
    diesel::insert_into(schema::sui_progress_store::table)
        .values(&SuiProgressStore {
            id: SUI_PROGRESS_STORE_DUMMY_KEY,
            txn_digest: tx_digest.inner().to_vec(),
        })
        .on_conflict(schema::sui_progress_store::dsl::id)
        .do_update()
        .set(txn_digest.eq(tx_digest.inner().to_vec()))
        .execute(&mut conn)
        .await?;
    Ok(())
}

pub async fn read_sui_progress_store(pool: &PgPool) -> anyhow::Result<Option<TransactionDigest>> {
    let mut conn = pool.get().await?;
    let val: Option<SuiProgressStore> = crate::schema::sui_progress_store::dsl::sui_progress_store
        .select(SuiProgressStore::as_select())
        .first(&mut conn)
        .await
        .optional()?;
    match val {
        Some(val) => Ok(Some(TransactionDigest::try_from(
            val.txn_digest.as_slice(),
        )?)),
        None => Ok(None),
    }
}

pub async fn get_latest_eth_token_transfer(
    pool: &PgPool,
    finalized: bool,
) -> Result<Option<DBTokenTransfer>, Error> {
    use crate::schema::token_transfer::dsl::*;

    let connection = &mut pool.get().await.unwrap();

    if finalized {
        token_transfer
            .filter(data_source.eq("ETH").and(status.eq("Deposited")))
            .order(block_height.desc())
            .first::<DBTokenTransfer>(connection)
            .await
            .optional()
    } else {
        token_transfer
            .filter(status.eq("DepositedUnfinalized"))
            .order(block_height.desc())
            .first::<DBTokenTransfer>(connection)
            .await
            .optional()
    }
}
