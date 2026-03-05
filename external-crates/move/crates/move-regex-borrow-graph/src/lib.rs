// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

pub mod collections;
pub(crate) mod graph_map;
pub mod meter;
pub mod references;
pub mod regex;

pub mod tests;

pub type Result<T> = std::result::Result<T, InvariantViolation>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantViolation(pub String);

pub type MeterResult<T, E> = std::result::Result<T, MeterError<E>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeterError<E> {
    Meter(E),
    InvariantViolation(InvariantViolation),
}

pub(crate) fn invariant_violation<E: From<InvariantViolation>>(msg: impl ToString) -> E {
    debug_assert!(false, "Invariant violation: {}", msg.to_string());
    InvariantViolation(msg.to_string()).into()
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

impl<E> From<InvariantViolation> for MeterError<E> {
    fn from(e: InvariantViolation) -> Self {
        MeterError::InvariantViolation(e)
    }
}

impl<E: Into<move_binary_format::errors::PartialVMError>> From<MeterError<E>>
    for move_binary_format::errors::PartialVMError
{
    fn from(e: MeterError<E>) -> Self {
        match e {
            MeterError::Meter(e) => e.into(),
            MeterError::InvariantViolation(e) => e.into(),
        }
    }
}

impl From<InvariantViolation> for move_binary_format::errors::PartialVMError {
    fn from(e: InvariantViolation) -> Self {
        debug_assert!(false, "Invariant violation: {}", e.0);
        move_binary_format::errors::PartialVMError::new(
            move_core_types::vm_status::StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
        )
        .with_message(e.0)
    }
}
