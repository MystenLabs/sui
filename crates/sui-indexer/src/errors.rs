// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::error::FastCryptoError;
use jsonrpsee::types::ErrorObjectOwned as RpcError;
use sui_name_service::NameServiceError;
use thiserror::Error;

use sui_types::base_types::ObjectIDParseError;
use sui_types::error::{SuiError, SuiObjectResponseError, UserInputError};

#[derive(Debug, Error)]
pub struct DataDownloadError {
    pub error: IndexerError,
    pub next_checkpoint_sequence_number: u64,
}

impl std::fmt::Display for DataDownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "next_checkpoint_seq: {}, error: {}",
            self.next_checkpoint_sequence_number, self.error
        )
    }
}

#[derive(Debug, Error)]
pub enum IndexerError {
    #[error("Indexer failed to read from archives store with error: `{0}`")]
    ArchiveReaderError(String),

    #[error("Stream closed unexpectedly with error: `{0}`")]
    ChannelClosed(String),

    #[error("Indexer failed to convert timestamp to NaiveDateTime with error: `{0}`")]
    DateTimeParsingError(String),

    #[error("Indexer failed to deserialize event from events table with error: `{0}`")]
    EventDeserializationError(String),

    #[error("Fullnode returns unexpected responses, which may block indexers from proceeding, with error: `{0}`")]
    UnexpectedFullnodeResponseError(String),

    #[error("Indexer failed to transform data with error: `{0}`")]
    DataTransformationError(String),

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

    #[error("Indexer error related to dynamic field: `{0}`")]
    DynamicFieldError(String),

    #[error("Indexer does not support the feature with error: `{0}`")]
    NotSupportedError(String),

    #[error("Indexer read corrupted/incompatible data from persistent storage: `{0}`")]
    PersistentStorageDataCorruptionError(String),

    #[error("Indexer generic error: `{0}`")]
    GenericError(String),

    #[error("GCS error: `{0}`")]
    GcsError(String),

    #[error("Indexer failed to resolve object to move struct with error: `{0}`")]
    ResolveMoveStructError(String),

    #[error(transparent)]
    UncategorizedError(#[from] anyhow::Error),

    #[error(transparent)]
    ObjectIdParseError(#[from] ObjectIDParseError),

    #[error("Invalid transaction digest with error: `{0}`")]
    InvalidTransactionDigestError(String),

    #[error(transparent)]
    SuiError(#[from] SuiError),

    #[error(transparent)]
    BcsError(#[from] bcs::Error),

    #[error("Invalid argument with error: `{0}`")]
    InvalidArgumentError(String),

    #[error(transparent)]
    UserInputError(#[from] UserInputError),

    #[error("Indexer failed to resolve module with error: `{0}`")]
    ModuleResolutionError(String),

    #[error(transparent)]
    ObjectResponseError(#[from] SuiObjectResponseError),

    #[error(transparent)]
    FastCryptoError(#[from] FastCryptoError),

    #[error("`{0}`: `{1}`")]
    ErrorWithContext(String, Box<IndexerError>),

    #[error("Indexer failed to send item to channel with error: `{0}`")]
    MpscChannelError(String),

    #[error(transparent)]
    NameServiceError(#[from] NameServiceError),

    #[error("Inconsistent migration records: {0}")]
    DbMigrationError(String),
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
        RpcError::owned(
            jsonrpsee::types::error::CALL_EXECUTION_FAILED_CODE,
            e.to_string(),
            None::<()>,
        )
    }
}

impl From<tokio::task::JoinError> for IndexerError {
    fn from(value: tokio::task::JoinError) -> Self {
        IndexerError::UncategorizedError(anyhow::Error::from(value))
    }
}

impl From<diesel_async::pooled_connection::bb8::RunError> for IndexerError {
    fn from(value: diesel_async::pooled_connection::bb8::RunError) -> Self {
        Self::PgPoolConnectionError(value.to_string())
    }
}

pub(crate) fn client_error_to_error_object(
    e: jsonrpsee::core::ClientError,
) -> jsonrpsee::types::ErrorObjectOwned {
    match e {
        jsonrpsee::core::ClientError::Call(e) => e,
        _ => jsonrpsee::types::ErrorObjectOwned::owned(
            jsonrpsee::types::error::UNKNOWN_ERROR_CODE,
            e.to_string(),
            None::<()>,
        ),
    }
}
