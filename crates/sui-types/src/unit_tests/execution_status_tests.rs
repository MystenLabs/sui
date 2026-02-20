// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_status::{ExecutionFailure, ExecutionFailureStatus, ExecutionStatus};
use serde::{Deserialize, Serialize};
use sui_enum_compat_util::*;

#[test]
fn enforce_order_test() {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "staged", "exec_failure_status.yaml"]);
    check_enum_compat_order::<ExecutionFailureStatus>(path);
}

#[test]
fn test_execution_status_serialization_stability() {
    // Test Case: Failure with InsufficientGas and no command index
    let status = ExecutionStatus::Failure(ExecutionFailure {
        error: ExecutionFailureStatus::InsufficientGas,
        command: None,
    });

    // 1. JSON Stability
    // Expected JSON format for the old struct-style variant was:
    // {"Failure": {"error": "InsufficientGas", "command": null}}
    // The new tuple-style variant containing a struct should produce the same JSON with default serde settings.
    let json_serialized = serde_json::to_string(&status).unwrap();
    let expected_json = "{\"Failure\":{\"error\":\"InsufficientGas\",\"command\":null}}";
    assert_eq!(
        json_serialized, expected_json,
        "JSON serialization changed! Expected {}, got {}",
        expected_json, json_serialized
    );

    // 2. BCS Stability
    // Expected BCS format:
    // Variant index for Failure: 1 (Success is 0)
    // ExecutionFailureStatus::InsufficientGas index: 0
    // Option<CommandIndex>::None: 0
    // Resulting bytes: [1, 0, 0]
    let bcs_serialized = bcs::to_bytes(&status).unwrap();
    let expected_bcs = vec![1, 0, 0];
    assert_eq!(
        bcs_serialized, expected_bcs,
        "BCS serialization changed! Expected {:?}, got {:?}",
        expected_bcs, bcs_serialized
    );

    // 3. Roundtrip
    let deserialized_json: ExecutionStatus = serde_json::from_str(&json_serialized).unwrap();
    assert_eq!(status, deserialized_json);

    let deserialized_bcs: ExecutionStatus = bcs::from_bytes(&bcs_serialized).unwrap();
    assert_eq!(status, deserialized_bcs);
}

#[test]
fn test_execution_status_with_command_serialization_stability() {
    // Test Case: Failure with InsufficientGas and command index 5
    let status = ExecutionStatus::Failure(ExecutionFailure {
        error: ExecutionFailureStatus::InsufficientGas,
        command: Some(5),
    });

    // 1. JSON Stability
    let json_serialized = serde_json::to_string(&status).unwrap();
    let expected_json = "{\"Failure\":{\"error\":\"InsufficientGas\",\"command\":5}}";
    assert_eq!(json_serialized, expected_json);

    // 2. BCS Stability
    // Variant index for Failure: 1
    // ExecutionFailureStatus::InsufficientGas index: 0
    // Option<CommandIndex>::Some(5): 1 followed by 5 (as u64 in BCS for usize usually, but let's check)
    let bcs_serialized = bcs::to_bytes(&status).unwrap();
    // [1 (Failure), 0 (InsufficientGas), 1 (Some), 5, 0, 0, 0, 0, 0, 0, 0 (5 as u64)]
    let mut expected_bcs = vec![1, 0, 1];
    expected_bcs.extend_from_slice(&5u64.to_le_bytes());
    assert_eq!(bcs_serialized, expected_bcs);
}

#[derive(Serialize, Deserialize)]
enum PrevExecutionStatus {
    Success,
    Failure {
        error: ExecutionFailureStatus,
        command: Option<usize>,
    },
}

#[test]
fn test_serialization_parity_with_previous_version() {
    use crate::execution_status::CommandArgumentError;
    use crate::execution_status::MoveLocation;
    use move_core_types::account_address::AccountAddress;
    use move_core_types::identifier::Identifier;
    use move_core_types::language_storage::ModuleId;

    let cases = vec![
        // Case 1: Success
        (
            ExecutionStatus::Success,
            PrevExecutionStatus::Success,
            "Success",
        ),
        // Case 2: Simple failure
        (
            ExecutionStatus::Failure(ExecutionFailure {
                error: ExecutionFailureStatus::InsufficientGas,
                command: None,
            }),
            PrevExecutionStatus::Failure {
                error: ExecutionFailureStatus::InsufficientGas,
                command: None,
            },
            "Simple Failure",
        ),
        // Case 3: Failure with command index
        (
            ExecutionStatus::Failure(ExecutionFailure {
                error: ExecutionFailureStatus::InsufficientGas,
                command: Some(123),
            }),
            PrevExecutionStatus::Failure {
                error: ExecutionFailureStatus::InsufficientGas,
                command: Some(123),
            },
            "Failure with Command Index",
        ),
        // Case 4: Complex failure (MoveAbort)
        (
            ExecutionStatus::Failure(ExecutionFailure {
                error: ExecutionFailureStatus::MoveAbort(
                    MoveLocation {
                        module: ModuleId::new(
                            AccountAddress::TWO,
                            Identifier::new("module").unwrap(),
                        ),
                        function: 1,
                        instruction: 2,
                        function_name: Some("func".to_string()),
                    },
                    456,
                ),
                command: Some(0),
            }),
            PrevExecutionStatus::Failure {
                error: ExecutionFailureStatus::MoveAbort(
                    MoveLocation {
                        module: ModuleId::new(
                            AccountAddress::TWO,
                            Identifier::new("module").unwrap(),
                        ),
                        function: 1,
                        instruction: 2,
                        function_name: Some("func".to_string()),
                    },
                    456,
                ),
                command: Some(0),
            },
            "Failure with MoveAbort",
        ),
        // Case 5: Failure with nested struct-style enum (CommandArgumentError)
        (
            ExecutionStatus::Failure(ExecutionFailure {
                error: ExecutionFailureStatus::CommandArgumentError {
                    arg_idx: 5,
                    kind: CommandArgumentError::TypeMismatch,
                },
                command: Some(1),
            }),
            PrevExecutionStatus::Failure {
                error: ExecutionFailureStatus::CommandArgumentError {
                    arg_idx: 5,
                    kind: CommandArgumentError::TypeMismatch,
                },
                command: Some(1),
            },
            "Failure with CommandArgumentError",
        ),
    ];

    for (current, prev, name) in cases {
        // 1. BCS Parity
        let current_bcs = bcs::to_bytes(&current).unwrap();
        let prev_bcs = bcs::to_bytes(&prev).unwrap();
        assert_eq!(
            current_bcs, prev_bcs,
            "BCS serialization parity failed for: {}",
            name
        );

        // 2. JSON Parity
        let current_json = serde_json::to_string(&current).unwrap();
        let prev_json = serde_json::to_string(&prev).unwrap();
        assert_eq!(
            current_json, prev_json,
            "JSON serialization parity failed for: {}",
            name
        );

        // 3. JSON content check (optional but good for debugging)
        let deserialized_from_prev: ExecutionStatus = serde_json::from_str(&prev_json).unwrap();
        assert_eq!(
            current, deserialized_from_prev,
            "Deserialization from prev JSON failed for: {}",
            name
        );
    }
}
