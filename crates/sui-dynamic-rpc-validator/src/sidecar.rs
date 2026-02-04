// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sidecar validation trait and macro for implementing dynamic RPC validators.
//!
//! This module provides a safe interface for implementing validation logic,
//! abstracting away the unsafe FFI boundary. Implementors define validation
//! logic using safe Rust code, and the provided macro generates the necessary
//! `extern "C"` exports.
//!
//! # Example
//!
//! ```ignore
//! use sui_dynamic_rpc_validator::sidecar::{RpcValidator, implement_validator_exports};
//!
//! struct MyValidator;
//!
//! impl RpcValidator for MyValidator {
//!     fn should_validate(&self) -> bool {
//!         true
//!     }
//!
//!     fn validate_submit_transaction(&self, message: &[u8]) -> bool {
//!         !message.is_empty()
//!     }
//!
//!     // Other methods use default implementations (accept all)
//! }
//!
//! implement_validator_exports!(MyValidator);
//! ```

/// Trait for implementing RPC validation logic in safe Rust.
///
/// All methods have default implementations that accept all messages,
/// so implementors only need to override the methods they care about.
pub trait RpcValidator: Default + Sync {
    /// Check if validation should be performed.
    ///
    /// Return `false` to enable fast path (skip all validation).
    /// Return `true` to perform validation.
    fn should_validate(&self) -> bool {
        true
    }

    /// Validate a submit_transaction RPC request.
    ///
    /// Return `true` to accept, `false` to reject.
    fn validate_submit_transaction(&self, message: &[u8]) -> bool {
        let _ = message;
        true
    }

    /// Validate a wait_for_effects RPC request.
    ///
    /// Return `true` to accept, `false` to reject.
    fn validate_wait_for_effects(&self, message: &[u8]) -> bool {
        let _ = message;
        true
    }

    /// Validate an object_info RPC request.
    ///
    /// Return `true` to accept, `false` to reject.
    fn validate_object_info(&self, message: &[u8]) -> bool {
        let _ = message;
        true
    }

    /// Validate a transaction_info RPC request.
    ///
    /// Return `true` to accept, `false` to reject.
    fn validate_transaction_info(&self, message: &[u8]) -> bool {
        let _ = message;
        true
    }

    /// Validate a checkpoint RPC request.
    ///
    /// Return `true` to accept, `false` to reject.
    fn validate_checkpoint(&self, message: &[u8]) -> bool {
        let _ = message;
        true
    }

    /// Validate a get_system_state_object RPC request.
    ///
    /// Return `true` to accept, `false` to reject.
    fn validate_system_state(&self, message: &[u8]) -> bool {
        let _ = message;
        true
    }

    /// Validate a validator_health RPC request.
    ///
    /// Return `true` to accept, `false` to reject.
    fn validate_validator_health(&self, message: &[u8]) -> bool {
        let _ = message;
        true
    }
}

/// Convert a raw pointer and length to a byte slice.
///
/// Returns `None` if the pointer is null.
///
/// # Safety
///
/// The caller must ensure that `ptr` points to valid memory of at least `len` bytes,
/// and that the memory remains valid for the lifetime of the returned slice.
#[inline]
pub unsafe fn ptr_to_slice<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if ptr.is_null() {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// Generate the extern "C" exports for a validator implementation.
///
/// This macro takes a type that implements [`RpcValidator`] and generates
/// all the necessary `extern "C"` functions that the dynamic loading system
/// expects.
///
/// The macro creates a static instance of the validator (using `Default`)
/// and delegates all FFI calls to the safe trait methods.
///
/// # Example
///
/// ```ignore
/// use sui_dynamic_rpc_validator::sidecar::{RpcValidator, implement_validator_exports};
///
/// #[derive(Default)]
/// struct MyValidator;
///
/// impl RpcValidator for MyValidator {
///     fn validate_submit_transaction(&self, message: &[u8]) -> bool {
///         // Your validation logic here
///         !message.is_empty()
///     }
/// }
///
/// implement_validator_exports!(MyValidator);
/// ```
#[macro_export]
macro_rules! implement_validator_exports {
    ($validator_type:ty) => {
        static VALIDATOR: std::sync::LazyLock<$validator_type> =
            std::sync::LazyLock::new(<$validator_type>::default);

        #[unsafe(no_mangle)]
        pub extern "C" fn should_validate() -> u8 {
            use $crate::sidecar::RpcValidator;
            if VALIDATOR.should_validate() { 1 } else { 0 }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn validate_submit_transaction(
            message_ptr: *const u8,
            message_len: usize,
        ) -> u8 {
            use $crate::sidecar::{RpcValidator, ptr_to_slice};
            let Some(message) = (unsafe { ptr_to_slice(message_ptr, message_len) }) else {
                return 0;
            };
            if VALIDATOR.validate_submit_transaction(message) {
                1
            } else {
                0
            }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn validate_wait_for_effects(
            message_ptr: *const u8,
            message_len: usize,
        ) -> u8 {
            use $crate::sidecar::{RpcValidator, ptr_to_slice};
            let Some(message) = (unsafe { ptr_to_slice(message_ptr, message_len) }) else {
                return 0;
            };
            if VALIDATOR.validate_wait_for_effects(message) {
                1
            } else {
                0
            }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn validate_object_info(
            message_ptr: *const u8,
            message_len: usize,
        ) -> u8 {
            use $crate::sidecar::{RpcValidator, ptr_to_slice};
            let Some(message) = (unsafe { ptr_to_slice(message_ptr, message_len) }) else {
                return 0;
            };
            if VALIDATOR.validate_object_info(message) {
                1
            } else {
                0
            }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn validate_transaction_info(
            message_ptr: *const u8,
            message_len: usize,
        ) -> u8 {
            use $crate::sidecar::{RpcValidator, ptr_to_slice};
            let Some(message) = (unsafe { ptr_to_slice(message_ptr, message_len) }) else {
                return 0;
            };
            if VALIDATOR.validate_transaction_info(message) {
                1
            } else {
                0
            }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn validate_checkpoint(
            message_ptr: *const u8,
            message_len: usize,
        ) -> u8 {
            use $crate::sidecar::{RpcValidator, ptr_to_slice};
            let Some(message) = (unsafe { ptr_to_slice(message_ptr, message_len) }) else {
                return 0;
            };
            if VALIDATOR.validate_checkpoint(message) {
                1
            } else {
                0
            }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn validate_system_state(
            message_ptr: *const u8,
            message_len: usize,
        ) -> u8 {
            use $crate::sidecar::{RpcValidator, ptr_to_slice};
            let Some(message) = (unsafe { ptr_to_slice(message_ptr, message_len) }) else {
                return 0;
            };
            if VALIDATOR.validate_system_state(message) {
                1
            } else {
                0
            }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn validate_validator_health(
            message_ptr: *const u8,
            message_len: usize,
        ) -> u8 {
            use $crate::sidecar::{RpcValidator, ptr_to_slice};
            let Some(message) = (unsafe { ptr_to_slice(message_ptr, message_len) }) else {
                return 0;
            };
            if VALIDATOR.validate_validator_health(message) {
                1
            } else {
                0
            }
        }
    };
}

pub use implement_validator_exports;
