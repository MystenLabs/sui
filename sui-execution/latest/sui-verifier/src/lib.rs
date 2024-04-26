// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod verifier;

pub mod entry_points_verifier;
pub mod global_storage_access_verifier;
pub mod id_leak_verifier;
pub mod meter;
pub mod one_time_witness_verifier;
pub mod private_generics;
pub mod struct_with_key_verifier;

use move_core_types::{ident_str, identifier::IdentStr, vm_status::StatusCode};
use sui_types::error::{ExecutionError, ExecutionErrorKind};

pub const INIT_FN_NAME: &IdentStr = ident_str!("init");
pub const TEST_SCENARIO_MODULE_NAME: &str = "test_scenario";

fn verification_failure(error: String) -> ExecutionError {
    ExecutionError::new_with_source(ExecutionErrorKind::SuiMoveVerificationError, error)
}

fn to_verification_timeout_error(error: String) -> ExecutionError {
    ExecutionError::new_with_source(ExecutionErrorKind::SuiMoveVerificationTimedout, error)
}

/// Runs the Move verifier and checks if the error counts as a Move verifier timeout
/// NOTE: this function only check if the verifier error is a timeout
/// All other errors are ignored
pub fn check_for_verifier_timeout(major_status_code: &StatusCode) -> bool {
    [
        StatusCode::PROGRAM_TOO_COMPLEX,
        // Do we want to make this a substatus of `PROGRAM_TOO_COMPLEX`?
        StatusCode::TOO_MANY_BACK_EDGES,
    ]
    .contains(major_status_code)
}
