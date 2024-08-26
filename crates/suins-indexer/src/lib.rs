// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod indexer;
pub mod models;
pub mod schema;

use dotenvy::dotenv;
use std::env;

use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;

pub type PgConnectionPool =
    diesel_async::pooled_connection::bb8::Pool<diesel_async::AsyncPgConnection>;
pub type PgPoolConnection<'a> =
    diesel_async::pooled_connection::bb8::PooledConnection<'a, AsyncPgConnection>;

pub async fn get_connection_pool() -> PgConnectionPool {
    //Pool<AsyncPgConnection> {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let manager =
        AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(database_url);

    Pool::builder()
        .build(manager)
        .await
        .expect("Could not build Postgres DB connection pool")
}
