// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

macro_rules! invariant_violation {
    ($msg:expr) => {{
        if cfg!(debug_assertions) {
            panic!("{}", $msg)
        }
        return Err(sui_types::error::ExecutionError::invariant_violation($msg).into());
    }};
}

macro_rules! assert_invariant {
    ($cond:expr, $msg:expr) => {{
        if !$cond {
            invariant_violation!($msg)
        }
    }};
}

pub mod adapter;
pub mod error;
pub mod execution_engine;
pub mod execution_mode;
pub mod programmable_transactions;
mod type_layout_resolver;
