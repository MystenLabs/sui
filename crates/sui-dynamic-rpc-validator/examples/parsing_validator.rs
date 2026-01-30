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
use sui_types::messages_grpc::{
    ObjectInfoRequest, RawSubmitTxRequest, RawValidatorHealthRequest, RawWaitForEffectsRequest,
    SystemStateRequest, TransactionInfoRequest,
};
use sui_types::transaction::Transaction;

/// Check if validation should be performed.
/// Returns 1 to indicate validation is needed.
#[unsafe(no_mangle)]
pub extern "C" fn should_validate() -> u8 {
    1
}

/// Helper to safely convert raw pointer and length to a byte slice.
///
/// # Safety
/// The caller must ensure that `ptr` points to valid memory of at least `len` bytes.
unsafe fn to_slice<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if ptr.is_null() {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// Validate a submit_transaction RPC request.
///
/// Expected format: Protobuf-encoded `RawSubmitTxRequest` containing BCS-encoded transactions.
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
        return 0;
    };

    // Validate each transaction in the request can be BCS-decoded
    for tx_bytes in &request.transactions {
        if bcs::from_bytes::<Transaction>(tx_bytes).is_err() {
            return 0;
        }
    }

    1
}

/// Validate a wait_for_effects RPC request.
///
/// Expected format: Protobuf-encoded `RawWaitForEffectsRequest`.
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_wait_for_effects(
    message_ptr: *const u8,
    message_len: usize,
) -> u8 {
    let Some(bytes) = (unsafe { to_slice(message_ptr, message_len) }) else {
        return 0;
    };

    // Try to decode the protobuf message
    if RawWaitForEffectsRequest::decode(bytes).is_err() {
        return 0;
    }

    1
}

/// Validate an object_info RPC request.
///
/// Expected format: BCS-encoded `ObjectInfoRequest`.
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_object_info(message_ptr: *const u8, message_len: usize) -> u8 {
    let Some(bytes) = (unsafe { to_slice(message_ptr, message_len) }) else {
        return 0;
    };

    // Try to BCS-decode the request
    if bcs::from_bytes::<ObjectInfoRequest>(bytes).is_err() {
        return 0;
    }

    1
}

/// Validate a transaction_info RPC request.
///
/// Expected format: BCS-encoded `TransactionInfoRequest`.
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_transaction_info(
    message_ptr: *const u8,
    message_len: usize,
) -> u8 {
    let Some(bytes) = (unsafe { to_slice(message_ptr, message_len) }) else {
        return 0;
    };

    // Try to BCS-decode the request
    if bcs::from_bytes::<TransactionInfoRequest>(bytes).is_err() {
        return 0;
    }

    1
}

/// Validate a checkpoint RPC request.
///
/// Expected format: BCS-encoded checkpoint sequence number (u64).
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_checkpoint(message_ptr: *const u8, message_len: usize) -> u8 {
    let Some(bytes) = (unsafe { to_slice(message_ptr, message_len) }) else {
        return 0;
    };

    // Checkpoint requests are typically just a sequence number
    if bcs::from_bytes::<u64>(bytes).is_err() {
        return 0;
    }

    1
}

/// Validate a get_system_state_object RPC request.
///
/// Expected format: BCS-encoded `SystemStateRequest`.
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_system_state(message_ptr: *const u8, message_len: usize) -> u8 {
    let Some(bytes) = (unsafe { to_slice(message_ptr, message_len) }) else {
        return 0;
    };

    // Try to BCS-decode the request
    if bcs::from_bytes::<SystemStateRequest>(bytes).is_err() {
        return 0;
    }

    1
}

/// Validate a validator_health RPC request.
///
/// Expected format: Protobuf-encoded `RawValidatorHealthRequest`.
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_validator_health(
    message_ptr: *const u8,
    message_len: usize,
) -> u8 {
    let Some(bytes) = (unsafe { to_slice(message_ptr, message_len) }) else {
        return 0;
    };

    // Try to decode the protobuf message
    if RawValidatorHealthRequest::decode(bytes).is_err() {
        return 0;
    }

    1
}
