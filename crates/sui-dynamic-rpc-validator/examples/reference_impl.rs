// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Reference implementation of dynamic RPC validator
//!
//! This is a simple example validator that accepts all non-empty requests.
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

use sui_dynamic_rpc_validator::implement_validator_exports;
use sui_dynamic_rpc_validator::sidecar::RpcValidator;

/// Reference validator that accepts all non-empty messages.
#[derive(Default)]
struct ReferenceValidator;

impl RpcValidator for ReferenceValidator {
    fn should_validate(&self) -> bool {
        true
    }

    fn validate_submit_transaction(&self, message: &[u8]) -> bool {
        !message.is_empty()
    }

    fn validate_wait_for_effects(&self, message: &[u8]) -> bool {
        !message.is_empty()
    }

    fn validate_object_info(&self, message: &[u8]) -> bool {
        !message.is_empty()
    }

    fn validate_transaction_info(&self, message: &[u8]) -> bool {
        !message.is_empty()
    }

    fn validate_checkpoint(&self, message: &[u8]) -> bool {
        !message.is_empty()
    }

    fn validate_system_state(&self, message: &[u8]) -> bool {
        !message.is_empty()
    }

    fn validate_validator_health(&self, message: &[u8]) -> bool {
        !message.is_empty()
    }
}

implement_validator_exports!(ReferenceValidator);
