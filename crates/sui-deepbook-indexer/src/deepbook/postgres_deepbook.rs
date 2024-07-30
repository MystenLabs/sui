// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{models::DeepbookType, postgres_manager::PgPool, schema::deepbook, schema::deep_price};
use diesel::{Connection, RunQueryDsl};

pub fn write(pool: &PgPool, data: Vec<DeepbookType>) -> Result<(), anyhow::Error> {
    let connection = &mut pool.get()?;
    connection.transaction(|conn| {
        for data in data {
            match data {
                DeepbookType::Deepbook(deepbook) => {
                    diesel::insert_into(deepbook::table)
                        .values(&deepbook)
                        .on_conflict_do_nothing()
                        .execute(conn)
                }
                DeepbookType::DeepPrice(deep_price) => {
                    diesel::insert_into(deep_price::table)
                        .values(&deep_price)
                        .on_conflict_do_nothing()
                        .execute(conn)
                }
            }?;
        }
        
        Ok(()) as Result<(), diesel::result::Error>
    })?;
    Ok(())
}
