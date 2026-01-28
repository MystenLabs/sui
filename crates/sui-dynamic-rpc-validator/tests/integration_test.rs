// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Integration test for loading the reference validator

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_dynamic_rpc_validator::{DynamicRpcValidator, DynamicValidatorMetrics, RpcMethod};

#[test]
#[ignore] // Ignore by default since it requires the reference validator to be built
fn test_load_reference_validator() {
    // Build the reference validator first:
    // cargo build --example reference_validator --release -p sui-dynamic-rpc-validator

    let lib_path = if cfg!(target_os = "macos") {
        "target/release/examples/libreference_validator.dylib"
    } else if cfg!(target_os = "linux") {
        "target/release/examples/libreference_validator.so"
    } else if cfg!(target_os = "windows") {
        "target/release/examples/reference_validator.dll"
    } else {
        panic!("Unsupported platform");
    };

    let full_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(lib_path);

    if !full_path.exists() {
        println!("Reference validator not found at: {:?}", full_path);
        println!("Build it with: cargo build --example reference_validator --release -p sui-dynamic-rpc-validator");
        return;
    }

    let metrics = Arc::new(DynamicValidatorMetrics::new(&prometheus::Registry::new()));
    let validator = DynamicRpcValidator::new(
        Some(full_path),
        Duration::from_secs(60),
        metrics.clone(),
    );

    // Test validation with the reference implementation
    assert!(validator.validate(RpcMethod::SubmitTransaction, b"test message"));
    assert!(!validator.validate(RpcMethod::SubmitTransaction, b"")); // Empty should be rejected

    // Check metrics
    assert_eq!(metrics.load_success.get(), 1);
    assert!(metrics.validation_success.with_label_values(&["submit_transaction"]).get() > 0);
}
