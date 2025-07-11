// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use sui_types::{
    effects::TransactionEvents, error::SuiError, messages_grpc::RawSubmitTxRequest, object::Object,
    quorum_driver_types::FinalizedEffects, transaction::Transaction,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SubmitTxRequest {
    pub transaction: Transaction,
}

impl SubmitTxRequest {
    pub fn into_raw(&self) -> Result<RawSubmitTxRequest, SuiError> {
        Ok(RawSubmitTxRequest {
            transaction: bcs::to_bytes(&self.transaction)
                .map_err(|e| SuiError::TransactionSerializationError {
                    error: e.to_string(),
                })?
                .into(),
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuorumTransactionResponse {
    // TODO(fastpath): Stop using QD types
    pub effects: FinalizedEffects,

    pub events: Option<TransactionEvents>,
    // Input objects will only be populated in the happy path
    pub input_objects: Option<Vec<Object>>,
    // Output objects will only be populated in the happy path
    pub output_objects: Option<Vec<Object>>,
    pub auxiliary_data: Option<Vec<u8>>,
}
