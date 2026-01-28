// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Reference implementation of dynamic RPC validator
//!
//! This is a simple example validator that accepts all requests.
//! Real implementations should perform actual validation logic.
//!
//! To build this as a shared object:
//! ```bash
//! cargo build --example reference_validator --release
//! ```
//!
//! The resulting shared object will be at:
//! `target/release/examples/libreference_validator.so` (Linux)
//! `target/release/examples/libreference_validator.dylib` (macOS)
//! `target/release/examples/reference_validator.dll` (Windows)

/// Check if validation should be performed
/// Returns 1 if validation is needed, 0 to use fast path (skip validation)
#[unsafe(no_mangle)]
pub extern "C" fn should_validate() -> u8 {
    // Return 1 to indicate that validation should be performed
    // Return 0 to skip all validation (fast path)
    1
}

/// Validate a submit_transaction RPC request
/// Returns 1 to accept, 0 to reject
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_submit_transaction(message_ptr: *const u8, message_len: usize) -> u8 {
    if message_ptr.is_null() {
        return 0; // Reject null messages
    }

    // SAFETY: The caller guarantees that message_ptr points to valid memory
    // of length message_len
    let _message_bytes = unsafe {
        std::slice::from_raw_parts(message_ptr, message_len)
    };

    // Example validation logic:
    // - Check message is not empty
    // - In a real implementation, you would parse and validate the message content

    if message_len == 0 {
        return 0; // Reject empty messages
    }

    // Accept all non-empty messages in this reference implementation
    1
}

/// Validate a wait_for_effects RPC request
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_wait_for_effects(message_ptr: *const u8, message_len: usize) -> u8 {
    if message_ptr.is_null() || message_len == 0 {
        return 0;
    }
    1 // Accept all valid messages
}

/// Validate an object_info RPC request
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_object_info(message_ptr: *const u8, message_len: usize) -> u8 {
    if message_ptr.is_null() || message_len == 0 {
        return 0;
    }
    1 // Accept all valid messages
}

/// Validate a transaction_info RPC request
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_transaction_info(message_ptr: *const u8, message_len: usize) -> u8 {
    if message_ptr.is_null() || message_len == 0 {
        return 0;
    }
    1 // Accept all valid messages
}

/// Validate a checkpoint RPC request
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_checkpoint(message_ptr: *const u8, message_len: usize) -> u8 {
    if message_ptr.is_null() || message_len == 0 {
        return 0;
    }
    1 // Accept all valid messages
}

/// Validate a get_system_state_object RPC request
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_system_state(message_ptr: *const u8, message_len: usize) -> u8 {
    if message_ptr.is_null() || message_len == 0 {
        return 0;
    }
    1 // Accept all valid messages
}

/// Validate a validator_health RPC request
///
/// # Safety
/// The caller must ensure that `message_ptr` points to valid memory of length `message_len`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_validator_health(message_ptr: *const u8, message_len: usize) -> u8 {
    if message_ptr.is_null() || message_len == 0 {
        return 0;
    }
    1 // Accept all valid messages
}
