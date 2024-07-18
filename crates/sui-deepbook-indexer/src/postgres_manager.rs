// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{models::Deepbook, schema::deepbook};
use diesel::{
    pg::PgConnection,
    r2d2::{ConnectionManager, Pool},
    Connection, RunQueryDsl,
};

pub type PgPool = Pool<ConnectionManager<PgConnection>>;

pub fn get_connection_pool(database_url: String) -> PgPool {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::builder()
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build Postgres DB connection pool")
}

pub fn write(pool: &PgPool, data: Vec<Deepbook>) -> Result<(), anyhow::Error> {
    let connection = &mut pool.get()?;
    connection.transaction(|conn| {
        diesel::insert_into(deepbook::table)
            .values(&data)
            .on_conflict_do_nothing()
            .execute(conn)
    })?;
    Ok(())
}
