// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    identifier::{IdentStr, Identifier},
    vm_status::StatusCode,
};

use lasso::{Spur, ThreadedRodeo};

/// A wrapper around a lasso ThreadedRoade with some niceties to make it easier to use in the VM.
#[derive(Debug)]
pub struct IdentifierInterner(ThreadedRodeo);

pub type IdentifierKey = Spur;

const STRING_SLOTS: usize = 1_000_000_000;

impl IdentifierInterner {
    pub fn new() -> Self {
        let rodeo = ThreadedRodeo::with_capacity(lasso::Capacity::for_strings(STRING_SLOTS));
        Self(rodeo)
    }

    /// Resolve a string in the interner or produce an invariant violation (as they should always be
    /// there). The `key_type` is used to make a more-informative error message.
    pub fn resolve_string(&self, key: &IdentifierKey, key_type: &str) -> PartialVMResult<String> {
        if let Some(result) = self.0.try_resolve(key) {
            Ok(result.to_string())
        } else {
            Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("Failed to find {key_type} key in string interner.")),
            )
        }
    }

    /// Get the interned identifier value. This may raise an invariant error if `try_get_or_intern`
    /// fails, but that's likely a serious OOM issue.
    pub fn get_or_intern_identifier(&self, ident: &Identifier) -> PartialVMResult<IdentifierKey> {
        self.get_or_intern_str(ident.borrow_str())
    }

    /// Get the interned identifier string value. This may raise an invariant error if
    /// `get_or_intern` fails, but that's likely a serious OOM issue.
    pub fn get_or_intern_ident_str(&self, ident_str: &IdentStr) -> PartialVMResult<IdentifierKey> {
        self.get_or_intern_str(ident_str.borrow_str())
    }

    /// Get the interned string value. This may raise an invariant error if `get_or_intern` fails,
    /// but that's likely a serious OOM issue.
    fn get_or_intern_str(&self, string: &str) -> PartialVMResult<IdentifierKey> {
        match self.0.try_get_or_intern(string) {
            Ok(result) => Ok(result),
            Err(err) => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("Failed to intern string {string}; error: {err:?}.")),
            ),
        }
    }

    /// Get the interned identifier value. This may raise an invariant error if `get` fails,
    /// which indicates the identifier was not interned.
    pub fn get_identifier(&self, ident: &Identifier) -> PartialVMResult<IdentifierKey> {
        self.get_str(ident.borrow_str())
    }

    /// Get the interned identifier string  value. This may raise an invariant error if `get`
    /// fails, which indicates the identifier string was not interned.
    pub fn get_ident_str(&self, ident_str: &IdentStr) -> PartialVMResult<IdentifierKey> {
        self.get_str(ident_str.borrow_str())
    }

    /// Get the interned string value. This may raise an invariant error if `get` fails, which
    /// indicates the identifier string was not interned.
    fn get_str(&self, string: &str) -> PartialVMResult<IdentifierKey> {
        match self.0.get(string) {
            Some(result) => Ok(result),
            None => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    format!(
                        "Failed to resolved identifer {string} \
                                          in string interner, which must be there."
                    ),
                ),
            ),
        }
    }
}
