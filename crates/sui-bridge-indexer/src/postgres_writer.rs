// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::TokenTransfer as DBTokenTransfer;
use crate::models::TokenTransferData as DBTokenTransferData;
use crate::schema::token_transfer_data;
use crate::{schema::token_transfer, TokenTransfer};
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

pub fn write(pool: &PgPool, token: TokenTransfer) {
    let connection = &mut pool.get().unwrap();
    connection
        .transaction(|conn| {
            if let Ok(data) = DBTokenTransferData::try_from(&token) {
                diesel::insert_into(token_transfer_data::table)
                    .values(data)
                    .on_conflict_do_nothing()
                    .execute(conn)?;
            };
            diesel::insert_into(token_transfer::table)
                .values(DBTokenTransfer::from(token))
                .on_conflict_do_nothing()
                .execute(conn)
        })
        .expect("Failed to start connection to DB");
}
