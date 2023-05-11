// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod verifier;

pub mod entry_points_verifier;
pub mod global_storage_access_verifier;
pub mod id_leak_verifier;
pub mod one_time_witness_verifier;
pub mod private_generics;
pub mod struct_with_key_verifier;

use move_core_types::{ident_str, identifier::IdentStr};
use sui_types::error::{ExecutionError, ExecutionErrorKind};

pub const INIT_FN_NAME: &IdentStr = ident_str!("init");
pub const TEST_SCENARIO_MODULE_NAME: &str = "test_scenario";

fn verification_failure(error: String) -> ExecutionError {
    ExecutionError::new_with_source(ExecutionErrorKind::SuiMoveVerificationError, error)
}
