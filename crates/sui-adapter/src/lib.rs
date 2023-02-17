// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

macro_rules! assert_invariant {
    ($cond:expr, $msg:expr) => {
        if !$cond {
            return Err(sui_types::error::ExecutionError::new_with_source(
                sui_types::error::ExecutionErrorKind::InvariantViolation,
                $msg,
            ));
        }
    };
}

pub mod adapter;
pub mod execution_engine;
pub mod execution_mode;
