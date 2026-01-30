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
use sui_types::messages_grpc::RawSubmitTxRequest;
use sui_types::transaction::Transaction;

/// Check if validation should be performed.
/// Returns 1 to indicate validation is needed.
#[unsafe(no_mangle)]
pub extern "C" fn should_validate() -> u8 {
    1
}

/// Helper to safely convert raw pointer and length to a byte slice.
unsafe fn to_slice<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if ptr.is_null() {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// Validate a submit_transaction RPC request.
/// Rejects transactions where the sender address ends in 0x00.
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_submit_transaction(
    message_ptr: *const u8,
    message_len: usize,
) -> u8 {
    let Some(bytes) = (unsafe { to_slice(message_ptr, message_len) }) else {
        return 0;
    };

    // Try to decode the outer protobuf message
    let Ok(request) = RawSubmitTxRequest::decode(bytes) else {
        // If we can't parse the request, reject it
        return 0;
    };

    // Check each transaction in the request
    for tx_bytes in &request.transactions {
        let Ok(transaction) = bcs::from_bytes::<Transaction>(tx_bytes) else {
            // If we can't parse the transaction, reject it
            return 0;
        };

        // Get the sender address
        let sender = transaction.sender_address();
        let sender_bytes = sender.to_inner();

        // Reject if the last byte of the sender address is 0x00
        if sender_bytes[31] == 0x00 {
            return 0;
        }
    }

    // Accept the transaction
    1
}

/// Accept all wait_for_effects requests (no filtering needed).
///
/// # Safety
/// The caller must ensure that `_message_ptr` points to valid memory of length `_message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_wait_for_effects(
    _message_ptr: *const u8,
    _message_len: usize,
) -> u8 {
    1
}

/// Accept all object_info requests (no filtering needed).
///
/// # Safety
/// The caller must ensure that `_message_ptr` points to valid memory of length `_message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_object_info(_message_ptr: *const u8, _message_len: usize) -> u8 {
    1
}

/// Accept all transaction_info requests (no filtering needed).
///
/// # Safety
/// The caller must ensure that `_message_ptr` points to valid memory of length `_message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_transaction_info(
    _message_ptr: *const u8,
    _message_len: usize,
) -> u8 {
    1
}

/// Accept all checkpoint requests (no filtering needed).
///
/// # Safety
/// The caller must ensure that `_message_ptr` points to valid memory of length `_message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_checkpoint(_message_ptr: *const u8, _message_len: usize) -> u8 {
    1
}

/// Accept all system_state requests (no filtering needed).
///
/// # Safety
/// The caller must ensure that `_message_ptr` points to valid memory of length `_message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_system_state(_message_ptr: *const u8, _message_len: usize) -> u8 {
    1
}

/// Accept all validator_health requests (no filtering needed).
///
/// # Safety
/// The caller must ensure that `_message_ptr` points to valid memory of length `_message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_validator_health(
    _message_ptr: *const u8,
    _message_len: usize,
) -> u8 {
    1
}
