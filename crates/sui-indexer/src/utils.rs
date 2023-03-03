// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::models::error_logs::{commit_error_logs, ErrorLog};
use crate::PgPoolConnection;

use tracing::error;

pub fn log_errors_to_pg(pg_pool_conn: &mut PgPoolConnection, errors: Vec<IndexerError>) {
    if errors.is_empty() {
        return;
    }
    let new_error_logs: Vec<ErrorLog> = errors.into_iter().map(|e| e.into()).collect();
    if let Err(e) = commit_error_logs(pg_pool_conn, new_error_logs) {
        error!("Failed writing error logs with error {:?}", e);
    }
}
