// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

pub mod collections;
pub mod references;
pub mod regex;

pub mod tests;

pub type Result<T> = std::result::Result<T, InvariantViolation>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantViolation(pub String);

pub(crate) fn invariant_violation(msg: impl ToString) -> InvariantViolation {
    debug_assert!(false, "Invariant violation: {}", msg.to_string());
    InvariantViolation(msg.to_string())
}

macro_rules! error {
    ($msg:expr $(,)?) => {
        $crate::invariant_violation($msg)
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::invariant_violation(format!($fmt, $($arg)*))
    };
}
pub(crate) use error;

macro_rules! bail {
    ($msg:expr $(,)?) => {
        return Err(error!($msg))
    };
    ($fmt:expr, $($arg:tt)*) => {
        return Err(error!($fmt, $($arg)*))
    };
}
pub(crate) use bail;

macro_rules! ensure {
    ($cond:expr, $msg:expr $(,)?) => {
        if !$cond {
            return Err(error!($msg))
        }
    };
    ($cond:expr, $fmt:expr, $($arg:tt)*) => {
        if !$cond {
            return Err(error!($fmt, $($arg)*))
        }
    };
}
pub(crate) use ensure;

impl From<InvariantViolation> for move_binary_format::errors::PartialVMError {
    fn from(e: InvariantViolation) -> Self {
        debug_assert!(false, "Invariant violation: {}", e.0);
        move_binary_format::errors::PartialVMError::new(
            move_core_types::vm_status::StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
        )
        .with_message(e.0)
    }
}
