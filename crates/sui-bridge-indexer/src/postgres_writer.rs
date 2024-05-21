// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::indexer::{models::TokenTxn, schema::tokens};
use diesel::{
    pg::PgConnection,
    r2d2::{ConnectionManager, Pool},
    Connection, RunQueryDsl,
};

pub(crate) type PgPool = Pool<ConnectionManager<PgConnection>>;

pub(crate) fn get_connection_pool(database_url: String) -> PgPool {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::builder()
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build Postgres DB connection pool")
}

pub(crate) fn write(pool: &PgPool, token: TokenTxn) {
    let connection = &mut pool.get().unwrap();
    connection
        .transaction(|conn| {
            diesel::insert_into(tokens::table)
                .values(token)
                .on_conflict_do_nothing()
                .execute(conn)
        })
        .expect("Failed to start connection to DB");
}
