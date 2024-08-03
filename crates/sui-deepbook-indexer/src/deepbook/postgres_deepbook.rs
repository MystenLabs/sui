// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{models::Deepbook, postgres_manager::PgPool, schema::deepbook};
use diesel::{Connection, RunQueryDsl};

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
