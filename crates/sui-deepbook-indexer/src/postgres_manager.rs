// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{
    balances, flashloans, order_fills, order_updates, pool_prices, proposals, rebates, stakes,
    sui_error_transactions, trade_params_update, votes,
};
use crate::types::ProcessedTxnData;
// use diesel::{
//     pg::PgConnection,
//     r2d2::{ConnectionManager, Pool},
//     RunQueryDsl,
// };
use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncConnection;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;

pub(crate) type PgPool =
    diesel_async::pooled_connection::bb8::Pool<diesel_async::AsyncPgConnection>;

pub async fn get_connection_pool(database_url: String) -> PgPool {
    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url);
    Pool::builder()
        .test_on_check_out(true)
        .build(manager)
        .await
        .expect("Could not build Postgres DB connection pool")
}

// TODO: add retry logic
pub async fn write(pool: &PgPool, txns: Vec<ProcessedTxnData>) -> Result<(), anyhow::Error> {
    if txns.is_empty() {
        return Ok(());
    }
    let (
        order_updates,
        order_fills,
        pool_prices,
        flahloans,
        balances,
        proposals,
        rebates,
        stakes,
        trade_params_update,
        votes,
        errors,
    ) = txns.iter().fold(
        (
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        ),
        |(
            mut order_updates,
            mut order_fills,
            mut pool_prices,
            mut flashloans,
            mut balances,
            mut proposals,
            mut rebates,
            mut stakes,
            mut trade_params_update,
            mut votes,
            mut errors,
        ),
         d| {
            match d {
                ProcessedTxnData::OrderUpdate(t) => {
                    order_updates.push(t.to_db());
                }
                ProcessedTxnData::OrderFill(t) => {
                    order_fills.push(t.to_db());
                }
                ProcessedTxnData::PoolPrice(t) => {
                    pool_prices.push(t.to_db());
                }
                ProcessedTxnData::Flashloan(t) => {
                    flashloans.push(t.to_db());
                }
                ProcessedTxnData::Balances(t) => {
                    balances.push(t.to_db());
                }
                ProcessedTxnData::Proposals(t) => {
                    proposals.push(t.to_db());
                }
                ProcessedTxnData::Rebates(t) => {
                    rebates.push(t.to_db());
                }
                ProcessedTxnData::Stakes(t) => {
                    stakes.push(t.to_db());
                }
                ProcessedTxnData::TradeParamsUpdate(t) => {
                    trade_params_update.push(t.to_db());
                }
                ProcessedTxnData::Votes(t) => {
                    votes.push(t.to_db());
                }
                ProcessedTxnData::Error(e) => errors.push(e.to_db()),
            }
            (
                order_updates,
                order_fills,
                pool_prices,
                flashloans,
                balances,
                proposals,
                rebates,
                stakes,
                trade_params_update,
                votes,
                errors,
            )
        },
    );

    let connection = &mut pool.get().await?;
    connection
        .transaction(|conn| {
            async move {
                diesel::insert_into(order_updates::table)
                    .values(&order_updates)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                diesel::insert_into(order_fills::table)
                    .values(&order_fills)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                diesel::insert_into(flashloans::table)
                    .values(&flahloans)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                diesel::insert_into(pool_prices::table)
                    .values(&pool_prices)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                diesel::insert_into(balances::table)
                    .values(&balances)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                diesel::insert_into(proposals::table)
                    .values(&proposals)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                diesel::insert_into(rebates::table)
                    .values(&rebates)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                diesel::insert_into(stakes::table)
                    .values(&stakes)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                diesel::insert_into(trade_params_update::table)
                    .values(&trade_params_update)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;
                diesel::insert_into(votes::table)
                    .values(&votes)
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
