// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    identifier::{IdentStr, Identifier},
    vm_status::StatusCode,
};

use lasso::{Spur, ThreadedRodeo};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// A wrapper around a lasso ThreadedRoade with some niceties to make it easier to use in the VM.
#[derive(Debug)]
pub struct IdentifierInterner(ThreadedRodeo);

/// Maximum number of identifiers we can ever intern.
/// FIXME: Set to 1 billion, but should be experimentally determined based on actual run data.
const IDENTIFIER_SLOTS: usize = 1_000_000_000;

// Note: these are not hashable or orderable -- their ordering is unstable, so they should not be
// used as keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
// Testing.
pub(crate) struct IdentifierKey(Spur);

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl IdentifierInterner {
    pub fn new() -> Self {
        let rodeo = ThreadedRodeo::with_capacity(lasso::Capacity::for_strings(IDENTIFIER_SLOTS));
        Self(rodeo)
    }

    // [SAFETY] The unsafe code is creating an identifier without checking its vailidity, but it
    // was added as a valid identifier to the interner in the first place.
    #[allow(unsafe_code)]
    /// Resolve an identifier in the interner or produce an invariant violation. This is for use
    /// when the key _must_ be there, as it produces an error when it is not found. The `key_type`
    /// is used to make a more-informative error message.
    pub(crate) fn resolve_ident(
        &self,
        key: &IdentifierKey,
        key_type: &str,
    ) -> PartialVMResult<Identifier> {
        if let Some(result) = self.0.try_resolve(&key.0) {
            unsafe { Ok(Identifier::new_unchecked(result)) }
        } else {
            Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("Failed to find {key_type} key in ident interner.")),
            )
        }
    }

    #[allow(dead_code)]
    /// Get the interned identifier value. This may raise an invariant error if `try_get_or_intern`
    /// fails, but that's likely a serious OOM issue.
    pub(crate) fn intern_identifier(&self, ident: &Identifier) -> PartialVMResult<IdentifierKey> {
        self.get_or_intern_str_internal(ident.borrow_str())
    }

    #[allow(dead_code)]
    /// Get the interned identifier string value. This may raise an invariant error if
    /// `get_or_intern` fails, but that's likely a serious OOM issue.
    pub(crate) fn intern_ident_str(&self, ident_str: &IdentStr) -> PartialVMResult<IdentifierKey> {
        self.get_or_intern_str_internal(ident_str.borrow_str())
    }

    #[allow(dead_code)]
    /// Get the interned identifier value, using `key_type` in the error case. This may raise an
    /// invariant error if `try_get_or_intern` fails, but that's likely a serious OOM issue.
    pub(crate) fn intern_identifier_with_msg(
        &self,
        ident: &Identifier,
        key_type: &str,
    ) -> PartialVMResult<IdentifierKey> {
        self.get_or_intern_str_internal(ident.borrow_str())
            .map_err(|err| err.with_message(format!("While attempting to intern {key_type}")))
    }

    fn get_or_intern_str_internal(&self, string: &str) -> PartialVMResult<IdentifierKey> {
        match self.0.try_get_or_intern(string) {
            Ok(result) => Ok(IdentifierKey(result)),
            Err(err) => Err(PartialVMError::new(StatusCode::INTERNER_LIMIT_REACHED)
                .with_message(format!("Failed to intern {string} ident; error: {err:?}."))),
        }
    }

    /// Get the current size of the interner in bytes.
    pub(crate) fn size(&self) -> usize {
        self.0.current_memory_usage()
    }
}
