// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Parsing validator implementation for dynamic RPC validation.
//!
//! This validator accepts messages if they can be successfully parsed according to
//! their expected format. It demonstrates how to implement actual validation logic
//! that checks message structure.
//!
//! # Message Formats
//!
//! The validator handles two types of message formats:
//! - **Protobuf (prost)**: Used by `SubmitTransaction`, `WaitForEffects`, and `ValidatorHealth`
//! - **BCS**: Used by `ObjectInfo`, `TransactionInfo`, `Checkpoint`, and `SystemState`
//!
//! # Building
//!
//! ```bash
//! cargo build --example parsing_validator --release --features parsing
//! ```
//!
//! The resulting shared object will be at:
//! - `target/release/examples/libparsing_validator.so` (Linux)
//! - `target/release/examples/libparsing_validator.dylib` (macOS)
//! - `target/release/examples/parsing_validator.dll` (Windows)

use prost::Message;
use sui_dynamic_rpc_validator::implement_validator_exports;
use sui_dynamic_rpc_validator::sidecar::RpcValidator;
use sui_types::messages_grpc::{
    ObjectInfoRequest, RawSubmitTxRequest, RawValidatorHealthRequest, RawWaitForEffectsRequest,
    SystemStateRequest, TransactionInfoRequest,
};
use sui_types::transaction::Transaction;

/// Parsing validator that accepts messages only if they can be successfully parsed.
#[derive(Default)]
struct ParsingValidator;

impl RpcValidator for ParsingValidator {
    fn should_validate(&self) -> bool {
        true
    }

    fn validate_submit_transaction(&self, message: &[u8]) -> bool {
        // Try to decode the outer protobuf message
        let Ok(request) = RawSubmitTxRequest::decode(message) else {
            return false;
        };

        // Validate each transaction in the request can be BCS-decoded
        for tx_bytes in &request.transactions {
            if bcs::from_bytes::<Transaction>(tx_bytes).is_err() {
                return false;
            }
        }

        true
    }

    fn validate_wait_for_effects(&self, message: &[u8]) -> bool {
        RawWaitForEffectsRequest::decode(message).is_ok()
    }

    fn validate_object_info(&self, message: &[u8]) -> bool {
        bcs::from_bytes::<ObjectInfoRequest>(message).is_ok()
    }

    fn validate_transaction_info(&self, message: &[u8]) -> bool {
        bcs::from_bytes::<TransactionInfoRequest>(message).is_ok()
    }

    fn validate_checkpoint(&self, message: &[u8]) -> bool {
        // Checkpoint requests are typically just a sequence number
        bcs::from_bytes::<u64>(message).is_ok()
    }

    fn validate_system_state(&self, message: &[u8]) -> bool {
        bcs::from_bytes::<SystemStateRequest>(message).is_ok()
    }

    fn validate_validator_health(&self, message: &[u8]) -> bool {
        RawValidatorHealthRequest::decode(message).is_ok()
    }
}

implement_validator_exports!(ParsingValidator);
