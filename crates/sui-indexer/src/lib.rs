// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::pg::PgConnection;
use diesel::prelude::*;

pub mod errors;
pub mod models;
pub mod schema;
pub mod utils;

pub fn establish_connection(db_url: String) -> PgConnection {
    PgConnection::establish(&db_url).unwrap_or_else(|_| panic!("Error connecting to {}", db_url))
}
