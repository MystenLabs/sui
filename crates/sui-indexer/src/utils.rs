// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::establish_connection;
use crate::models::error_logs::{commit_error_logs, err_to_error_log, NewErrorLog};

use tracing::error;

pub fn log_errors_to_pg(errors: Vec<IndexerError>) {
    let mut pg_conn = establish_connection();
    let new_error_logs: Vec<NewErrorLog> = errors.into_iter().map(err_to_error_log).collect();
    if let Err(e) = commit_error_logs(&mut pg_conn, new_error_logs) {
        error!("Failed writing error logs with error {:?}", e);
    }
}
