// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IndexerError {
    #[error("Indexer cannot read fullnode with error: `{0}`")]
    FullNodeReadingError(String),

    #[error("Indexer cannot read PostgresDB with error: `{0}`")]
    PostgresReadError(String),

    #[error("Indexer failed commiting changes to PostgresDB with error: `{0}`")]
    PostgresWriteError(String),

    #[error("Indexer failed converting to diesel Insertable with error: `{0}`")]
    InsertableParsingError(String),

    #[error("Indexer failed to convert timestamp to NaiveDateTime with error: `{0}`")]
    DateTimeParsingError(String),

    #[error("Indexer failed to parse transaction digest read from DB: `{0}`")]
    TransactionDigestParsingError(String),
}

impl IndexerError {
    pub fn name(&self) -> String {
        match self {
            IndexerError::FullNodeReadingError(_) => "FullNodeReadingError".to_string(),
            IndexerError::PostgresReadError(_) => "PostgresReadError".to_string(),
            IndexerError::PostgresWriteError(_) => "PostgresWriteError".to_string(),
            IndexerError::InsertableParsingError(_) => "InsertableParsingError".to_string(),
            IndexerError::DateTimeParsingError(_) => "DateTimeParsingError".to_string(),
            IndexerError::TransactionDigestParsingError(_) => {
                "TransactionDigestParsingError".to_string()
            }
        }
    }
}
