// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Bug detection oracles for the Sui Move VM fuzzer.
//!
//! Provides wrappers to detect panics (validator crashes) and inspect
//! transaction effects for signs of invariant violations or fund loss.

use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Debug)]
pub enum BugClass {
    ValidatorCrash {
        pass: String,
        message: String,
    },
    FundLoss {
        details: String,
    },
    VerifierSoundness {
        description: String,
    },
}

/// Wrap a closure in `catch_unwind`. Returns `Err(BugClass::ValidatorCrash)` on panic.
pub fn check_crash<F: FnOnce() -> R, R>(label: &str, f: F) -> Result<R, BugClass> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => Ok(result),
        Err(payload) => {
            let message = if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else {
                "unknown panic".to_string()
            };
            Err(BugClass::ValidatorCrash {
                pass: label.to_string(),
                message,
            })
        }
    }
}

/// Check `TransactionEffects` for signs of invariant violations or fund loss.
/// Returns `Some(BugClass)` if suspicious.
#[cfg(feature = "e2e")]
pub fn check_effects_for_bugs(
    effects: &sui_types::effects::TransactionEffects,
) -> Option<BugClass> {
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};

    match effects.status() {
        ExecutionStatus::Failure { error, .. } => match error {
            ExecutionFailureStatus::InvariantViolation => Some(BugClass::VerifierSoundness {
                description: "InvariantViolation in transaction effects".to_string(),
            }),
            ExecutionFailureStatus::VMInvariantViolation => Some(BugClass::VerifierSoundness {
                description: "VMInvariantViolation in transaction effects".to_string(),
            }),
            _ => None,
        },
        ExecutionStatus::Success => None,
    }
}
