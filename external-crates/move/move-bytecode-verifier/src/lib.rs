// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

//! Verifies bytecode sanity.

// Bounds checks are implemented in the `vm` crate.
pub mod ability_field_requirements;
pub mod absint;
pub mod check_duplication;
pub mod code_unit_verifier;
pub mod constants;
pub mod control_flow;
pub mod control_flow_v5;
pub mod cyclic_dependencies;
pub mod dependencies;
pub mod friends;
pub mod instantiation_loops;
pub mod instruction_consistency;
pub mod limits;
pub mod loop_summary;
pub mod script_signature;
pub mod signature;
pub mod struct_defs;
pub mod verifier;

pub use check_duplication::DuplicationChecker;
pub use code_unit_verifier::CodeUnitVerifier;
pub use instruction_consistency::InstructionConsistency;
pub use script_signature::{
    legacy_script_signature_checks, no_additional_script_signature_checks, FnCheckScriptSignature,
};
pub use signature::SignatureChecker;
pub use struct_defs::RecursiveStructDefChecker;
pub use verifier::{
    verify_module_unmetered, verify_module_with_config_for_test, verify_module_with_config_metered,
    verify_module_with_config_unmetered, verify_script_unmetered,
    verify_script_with_config_metered, verify_script_with_config_unmetered, VerifierConfig,
};

mod acquires_list_verifier;
mod locals_safety;
pub mod meter;
mod reference_safety;
mod regression_tests;
mod stack_usage_verifier;
mod type_safety;
