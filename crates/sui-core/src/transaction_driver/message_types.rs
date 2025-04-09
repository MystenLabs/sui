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

    pub include_events: bool,
    pub include_input_objects: bool,
    pub include_output_objects: bool,
    pub include_auxiliary_data: bool,
}

impl SubmitTxRequest {
    pub fn into_raw(&self) -> Result<RawSubmitTxRequest, SuiError> {
        Ok(RawSubmitTxRequest {
            transaction: bcs::to_bytes(&self.transaction)
                .map_err(|e| SuiError::TransactionSerializationError {
                    error: e.to_string(),
                })?
                .into(),
            include_events: self.include_events,
            include_input_objects: self.include_input_objects,
            include_output_objects: self.include_output_objects,
            include_auxiliary_data: self.include_auxiliary_data,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuorumSubmitTransactionResponse {
    // TODO(fastpath): Stop using QD types
    pub effects: FinalizedEffects,

    pub events: Option<TransactionEvents>,
    // Input objects will only be populated in the happy path
    pub input_objects: Option<Vec<Object>>,
    // Output objects will only be populated in the happy path
    pub output_objects: Option<Vec<Object>>,
    pub auxiliary_data: Option<Vec<u8>>,
}
