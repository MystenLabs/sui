// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod indexer;
pub mod models;
pub mod schema;

use dotenvy::dotenv;
use std::env;

use diesel::{ConnectionError, ConnectionResult};
use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::ManagerConfig;
use diesel_async::AsyncPgConnection;
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use std::time::Duration;

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

    let pg_pool = Pool::builder()
        .connection_timeout(Duration::from_secs(30))
        .build(manager)
        .await
        .expect("Could not build Postgres DB connection pool");

    match pg_pool.get().await {
        Ok(_conn) => {
            // If connection is successfully acquired, the pool is healthy
            println!("Connection is healthy.");
        }
        Err(e) => {
            // If there is an error, return it or handle it as a failure
            eprintln!("Failed to get a connection: {}", e);
        }
    }

    pg_pool
}

pub async fn get_connection_pool_w_tls() -> PgConnectionPool {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let mut config = ManagerConfig::default();
    config.custom_setup = Box::new(establish_connection);

    let manager = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new_with_config(
        database_url,
        config,
    );

    let pg_pool = Pool::builder()
        .max_size(10)
        .connection_timeout(Duration::from_secs(30))
        .build(manager)
        .await
        .expect("Could not build Postgres DB connection pool");

    match pg_pool.get().await {
        Ok(_conn) => {
            // If connection is successfully acquired, the pool is healthy
            println!("Connection is healthy.");
        }
        Err(e) => {
            // If there is an error, return it or handle it as a failure
            eprintln!("Failed to get a connection: {}", e);
        }
    }

    pg_pool
}

fn establish_connection(config: &str) -> BoxFuture<ConnectionResult<AsyncPgConnection>> {
    let fut = async {
        // We first set up the way we want rustls to work.
        let rustls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_certs())
            .with_no_client_auth();
        let tls = tokio_postgres_rustls::MakeRustlsConnect::new(rustls_config);
        let (client, conn) = tokio_postgres::connect(config, tls)
            .await
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("Database connection: {e}");
            }
        });
        AsyncPgConnection::try_from(client).await
    };
    fut.boxed()
}

fn root_certs() -> rustls::RootCertStore {
    let mut roots = rustls::RootCertStore::empty();
    let certs = rustls_native_certs::load_native_certs().expect("Certs not loadable!");
    roots.add_parsable_certificates(certs);
    roots
}
