// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IndexerError {
    #[error("Indexer failed to convert timestamp to NaiveDateTime with error: `{0}`")]
    DateTimeParsingError(String),

    #[error("Indexer failed to deserialize event from events table with error: `{0}`")]
    EventDeserializationError(String),

    #[error("Indexer failed to read fullnode with error: `{0}`")]
    FullNodeReadingError(String),

    #[error("Indexer failed to convert structs to diesel Insertable with error: `{0}`")]
    InsertableParsingError(String),

    #[error("Indexer failed to find object mutations, which should never happen.")]
    ObjectMutationNotAvailable,

    #[error("Indexer failed to build PG connection pool with error: `{0}`")]
    PgConnectionPoolInitError(String),

    #[error("Indexer failed to get a pool connection from PG connection pool with error: `{0}`")]
    PgPoolConnectionError(String),

    #[error("Indexer failed to read PostgresDB with error: `{0}`")]
    PostgresReadError(String),

    #[error("Indexer failed to commit changes to PostgresDB with error: `{0}`")]
    PostgresWriteError(String),

    #[error("Indexer failed to initialize fullnode RPC client with error: `{0}`")]
    RpcClientInitError(String),

    #[error("Indexer failed to convert timestamp to NaiveDateTime.")]
    TimestampOverflow,

    #[error("Indexer failed to parse transaction digest read from DB with error: `{0}`")]
    TransactionDigestParsingError(String),

    #[error("Indexer failed to find transaction time, which should not happen.")]
    TransactionTimeNotAvailable,
}

impl IndexerError {
    pub fn name(&self) -> String {
        match self {
            IndexerError::FullNodeReadingError(_) => "FullNodeReadingError".into(),
            IndexerError::PostgresReadError(_) => "PostgresReadError".into(),
            IndexerError::PostgresWriteError(_) => "PostgresWriteError".into(),
            IndexerError::InsertableParsingError(_) => "InsertableParsingError".into(),
            IndexerError::DateTimeParsingError(_) => "DateTimeParsingError".into(),
            IndexerError::TransactionDigestParsingError(_) => {
                "TransactionDigestParsingError".into()
            }
            IndexerError::ObjectMutationNotAvailable => "ObjectMutationNotAvailable".into(),
            IndexerError::EventDeserializationError(_) => "EventDeserializationError".into(),
            IndexerError::PgConnectionPoolInitError(_) => "PgConnectionPoolInitError".into(),
            IndexerError::RpcClientInitError(_) => "RpcClientInitError".into(),
            IndexerError::PgPoolConnectionError(_) => "PgPoolConnectionError".into(),
            IndexerError::TransactionTimeNotAvailable => "TransactionTimeNotAvailable".into(),
            IndexerError::TimestampOverflow => "TimestampOverflow".into(),
        }
    }
}
