// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::TokenTransfer as DBTokenTransfer;
use crate::models::TokenTransferData as DBTokenTransferData;
use crate::schema::token_transfer_data;
use crate::{schema::token_transfer, TokenTransfer};
use diesel::result::Error;
use diesel::BoolExpressionMethods;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::QueryDsl;
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
