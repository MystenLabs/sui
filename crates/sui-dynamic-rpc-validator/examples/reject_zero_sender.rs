// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Test validator that rejects transactions if the sender address ends in zero.
//!
//! This validator is used for testing the dynamic RPC validation system.
//! It demonstrates how validators can apply custom filtering rules.
//!
//! # Building
//!
//! ```bash
//! cargo build --example reject_zero_sender --release --features parsing
//! ```
//!
//! The resulting shared object will be at:
//! - `target/release/examples/libreject_zero_sender.so` (Linux)
//! - `target/release/examples/libreject_zero_sender.dylib` (macOS)
//! - `target/release/examples/reject_zero_sender.dll` (Windows)

use prost::Message;
use sui_dynamic_rpc_validator::implement_validator_exports;
use sui_dynamic_rpc_validator::sidecar::RpcValidator;
use sui_types::messages_grpc::RawSubmitTxRequest;
use sui_types::transaction::Transaction;

/// Validator that rejects transactions where the sender address ends in 0x00.
#[derive(Default)]
struct RejectZeroSenderValidator;

impl RpcValidator for RejectZeroSenderValidator {
    fn should_validate(&self) -> bool {
        true
    }

    fn validate_submit_transaction(&self, message: &[u8]) -> bool {
        // Try to decode the outer protobuf message
        let Ok(request) = RawSubmitTxRequest::decode(message) else {
            // If we can't parse the request, reject it
            return false;
        };

        // Check each transaction in the request
        for tx_bytes in &request.transactions {
            let Ok(transaction) = bcs::from_bytes::<Transaction>(tx_bytes) else {
                // If we can't parse the transaction, reject it
                return false;
            };

            // Get the sender address
            let sender = transaction.sender_address();
            let sender_bytes = sender.to_inner();

            // Reject if the last byte of the sender address is 0x00
            if sender_bytes[31] == 0x00 {
                return false;
            }
        }

        // Accept the transaction
        true
    }

    // All other RPC methods use the default implementation (accept all)
}

implement_validator_exports!(RejectZeroSenderValidator);
