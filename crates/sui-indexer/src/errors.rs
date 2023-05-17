// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::error::FastCryptoError;
use jsonrpsee::core::Error as RpcError;
use jsonrpsee::types::error::CallError;
use thiserror::Error;

use sui_types::base_types::ObjectIDParseError;
use sui_types::error::{SuiError, SuiObjectResponseError, UserInputError};

#[derive(Debug, Error)]
pub enum IndexerError {
    #[error("Indexer failed to convert timestamp to NaiveDateTime with error: `{0}`")]
    DateTimeParsingError(String),

    #[error("Indexer failed to deserialize event from events table with error: `{0}`")]
    EventDeserializationError(String),

    #[error("Fullnode returns unexpected responses, which may block indexers from proceeding, with error: `{0}`")]
    UnexpectedFullnodeResponseError(String),

    #[error("Indexer failed to read fullnode with error: `{0}`")]
    FullNodeReadingError(String),

    #[error("Indexer failed to convert structs to diesel Insertable with error: `{0}`")]
    InsertableParsingError(String),

    #[error("Indexer failed to build JsonRpcServer with error: `{0}`")]
    JsonRpcServerError(#[from] sui_json_rpc::error::Error),

    #[error("Indexer failed to find object mutations, which should never happen.")]
    ObjectMutationNotAvailable,

    #[error("Indexer failed to build PG connection pool with error: `{0}`")]
    PgConnectionPoolInitError(String),

    #[error("Indexer failed to get a pool connection from PG connection pool with error: `{0}`")]
    PgPoolConnectionError(String),

    #[error("Indexer failed to read PostgresDB with error: `{0}`")]
    PostgresReadError(String),

    #[error("Indexer failed to reset PostgresDB with error: `{0}`")]
    PostgresResetError(String),

    #[error("Indexer failed to commit changes to PostgresDB with error: `{0}`")]
    PostgresWriteError(String),

    #[error(transparent)]
    PostgresError(#[from] diesel::result::Error),

    #[error("Indexer failed to initialize fullnode Http client with error: `{0}`")]
    HttpClientInitError(String),

    #[error("Indexer failed to serialize/deserialize with error: `{0}`")]
    SerdeError(String),

    #[error("Indexer does not support the feature with error: `{0}`")]
    NotSupportedError(String),

    #[error(transparent)]
    UncategorizedError(#[from] anyhow::Error),

    #[error(transparent)]
    ObjectIdParseError(#[from] ObjectIDParseError),

    #[error(transparent)]
    SuiError(#[from] SuiError),

    #[error(transparent)]
    BcsError(#[from] bcs::Error),

    #[error("Invalid argument with error: `{0}`")]
    InvalidArgumentError(String),

    #[error(transparent)]
    UserInputError(#[from] UserInputError),

    #[error(transparent)]
    ObjectResponseError(#[from] SuiObjectResponseError),

    #[error(transparent)]
    FastCryptoError(#[from] FastCryptoError),

    #[error("`{0}`: `{1}`")]
    ErrorWithContext(String, Box<IndexerError>),

    #[error("Indexer failed to send item to channel with error: `{0}`")]
    MpscChannelError(String),
}

pub trait Context<T> {
    fn context(self, context: &str) -> Result<T, IndexerError>;
}

impl<T> Context<T> for Result<T, IndexerError> {
    fn context(self, context: &str) -> Result<T, IndexerError> {
        self.map_err(|e| IndexerError::ErrorWithContext(context.to_string(), Box::new(e)))
    }
}

impl From<IndexerError> for RpcError {
    fn from(e: IndexerError) -> Self {
        RpcError::Call(CallError::Failed(e.into()))
    }
}
