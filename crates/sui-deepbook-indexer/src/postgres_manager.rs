// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{flashloans, order_fills, order_updates, pool_prices, sui_error_transactions};
use crate::ProcessedTxnData;
use diesel::{
    pg::PgConnection,
    r2d2::{ConnectionManager, Pool},
    Connection, RunQueryDsl,
};

pub(crate) type PgPool = Pool<ConnectionManager<PgConnection>>;

pub fn get_connection_pool(database_url: String) -> PgPool {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::builder()
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build Postgres DB connection pool")
}

// TODO: add retry logic
pub fn write(pool: &PgPool, txns: Vec<ProcessedTxnData>) -> Result<(), anyhow::Error> {
    if txns.is_empty() {
        return Ok(());
    }
    let (order_updates, order_fills, pool_prices, flahloans, errors) = txns.iter().fold(
        (vec![], vec![], vec![], vec![], vec![]),
        |(mut order_updates, mut order_fills, mut pool_prices, mut flashloans, mut errors), d| {
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
                ProcessedTxnData::Error(e) => errors.push(e.to_db()),
            }
            (order_updates, order_fills, pool_prices, flashloans, errors)
        },
    );

    let connection = &mut pool.get()?;
    connection.transaction(|conn| {
        diesel::insert_into(order_updates::table)
            .values(&order_updates)
            .on_conflict_do_nothing()
            .execute(conn)?;
        diesel::insert_into(order_fills::table)
            .values(&order_fills)
            .on_conflict_do_nothing()
            .execute(conn)?;
        diesel::insert_into(flashloans::table)
            .values(&flahloans)
            .on_conflict_do_nothing()
            .execute(conn)?;
        diesel::insert_into(pool_prices::table)
            .values(&pool_prices)
            .on_conflict_do_nothing()
            .execute(conn)?;
        diesel::insert_into(sui_error_transactions::table)
            .values(&errors)
            .on_conflict_do_nothing()
            .execute(conn)
    })?;
    Ok(())
}
