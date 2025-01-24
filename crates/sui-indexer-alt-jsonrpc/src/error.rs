// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//

//! # Error
//!
//! Helper functions for propagating errors from within the service as JSON-RPC errors. Components
//! in the service may return errors in a variety of types. Bodies of JSON-RPC method handlers are
//! responsible for assigning an error code for these errors.

use std::fmt::Display;

use jsonrpsee::types::{
    error::{INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE},
    ErrorObject,
};

/// Macro wrapping a call to an RPC error constructor (above) which adds support for format
/// strings, and immediate early return:
///
///  rpc_bail!(internal_error("hello, {}", "world"))
///
/// Becomes
///
///  return Err(internal_error(format!("hello, {}", "world")))
macro_rules! rpc_bail {
    ($kind:ident ( $fmt:literal $(,$x:expr)* $(,)? ) ) => {
        return Err(crate::error::$kind(format!($fmt, $($x),*)))
    };
}

pub(crate) use rpc_bail;

pub(crate) fn internal_error(err: impl ToString) -> ErrorObject<'static> {
    ErrorObject::owned(INTERNAL_ERROR_CODE, err.to_string(), None::<()>)
}

pub(crate) fn invalid_params(err: impl ToString) -> ErrorObject<'static> {
    ErrorObject::owned(INVALID_PARAMS_CODE, err.to_string(), None::<()>)
}

pub(crate) fn pruned(what: impl Display) -> ErrorObject<'static> {
    ErrorObject::owned(INVALID_PARAMS_CODE, format!("{what} pruned"), None::<()>)
}
