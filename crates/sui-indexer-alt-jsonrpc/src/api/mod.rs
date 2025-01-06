// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::dsl::Limit;
use diesel::pg::Pg;
use diesel::query_builder::QueryFragment;
use diesel::query_dsl::methods::LimitDsl;
use diesel::result::Error as DieselError;
use diesel_async::methods::LoadQuery;
use diesel_async::RunQueryDsl;
use jsonrpsee::types::{error::INTERNAL_ERROR_CODE, ErrorObject};
use sui_pg_db as db;
use tracing::debug;

pub(crate) mod governance;

/// This wrapper type exists to perform error conversion between the data fetching layer and the
/// RPC layer, and to handle debug logging of database queries.
struct Connection<'p>(db::Connection<'p>);

#[derive(thiserror::Error, Debug)]
enum DbError {
    #[error(transparent)]
    Connect(anyhow::Error),

    #[error(transparent)]
    RunQuery(#[from] DieselError),
}

impl<'p> Connection<'p> {
    async fn get(db: &'p db::Db) -> Result<Self, DbError> {
        Ok(Self(db.connect().await.map_err(DbError::Connect)?))
    }

    async fn first<'q, Q, U>(&mut self, query: Q) -> Result<U, DbError>
    where
        U: Send,
        Q: RunQueryDsl<db::ManagedConnection> + 'q,
        Q: LimitDsl,
        Limit<Q>: LoadQuery<'q, db::ManagedConnection, U> + QueryFragment<Pg> + Send,
    {
        let query = query.limit(1);
        debug!("{}", diesel::debug_query(&query));
        Ok(query.get_result(&mut self.0).await?)
    }
}

impl From<DbError> for ErrorObject<'static> {
    fn from(err: DbError) -> Self {
        match err {
            DbError::Connect(err) => {
                ErrorObject::owned(INTERNAL_ERROR_CODE, err.to_string(), None::<()>)
            }

            DbError::RunQuery(err) => {
                ErrorObject::owned(INTERNAL_ERROR_CODE, err.to_string(), None::<()>)
            }
        }
    }
}
