// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use jsonrpsee::types::error::UNKNOWN_ERROR_CODE;
pub use sui_json_rpc_api::{TRANSACTION_EXECUTION_CLIENT_ERROR_CODE, TRANSIENT_ERROR_CODE};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub struct Error {
    pub code: i32,
    pub message: String,
    // TODO: as this SDK is specialized for the Sui JSON RPC implementation, we should define structured representation for the data field if applicable
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "code: '{}', message: '{}'", self.code, self.message)
    }
}

impl Error {
    pub fn is_call_error(&self) -> bool {
        self.code != UNKNOWN_ERROR_CODE
    }

    pub fn is_client_error(&self) -> bool {
        use jsonrpsee::types::error::{
            BATCHES_NOT_SUPPORTED_CODE, INVALID_PARAMS_CODE, INVALID_REQUEST_CODE,
            METHOD_NOT_FOUND_CODE, OVERSIZED_REQUEST_CODE, PARSE_ERROR_CODE,
        };
        matches!(
            self.code,
            PARSE_ERROR_CODE
                | OVERSIZED_REQUEST_CODE
                | INVALID_PARAMS_CODE
                | INVALID_REQUEST_CODE
                | METHOD_NOT_FOUND_CODE
                | BATCHES_NOT_SUPPORTED_CODE
                | TRANSACTION_EXECUTION_CLIENT_ERROR_CODE
        )
    }

    pub fn is_execution_error(&self) -> bool {
        self.code == TRANSACTION_EXECUTION_CLIENT_ERROR_CODE
    }

    pub fn is_transient_error(&self) -> bool {
        self.code == TRANSIENT_ERROR_CODE
    }
}

impl From<jsonrpsee::core::ClientError> for Error {
    fn from(err: jsonrpsee::core::ClientError) -> Self {
        // The following code relies on jsonrpsee's From<Error> for ErrorObjectOwned implementation
        // It converts any variant that is not Error::Call into an ErrorObject with UNKNOWN_ERROR_CODE
        let error_object_owned = match err {
            jsonrpsee::core::ClientError::Call(e) => e,
            _ => jsonrpsee::types::error::ErrorCode::from(UNKNOWN_ERROR_CODE).into(),
        };
        Error {
            code: error_object_owned.code(),
            message: error_object_owned.message().to_string(),
            data: None,
        }
    }
}
