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

/// Maximum number of identifiers we can ever intern.
/// FIXME: Set to 1 billion, but should be experimentally determined based on actual run data.
const IDENTIFIER_SLOTS: usize = 1_000_000_000;

pub type IdentifierKey = Spur;

impl IdentifierInterner {
    pub fn new() -> Self {
        let rodeo = ThreadedRodeo::with_capacity(lasso::Capacity::for_strings(IDENTIFIER_SLOTS));
        Self(rodeo)
    }

    /// Resolve an identifier in the interner or produce an invariant violation (as they should
    /// always be there). The `key_type` is used to make a more-informative error message. The
    /// unsafe code is creating an identifier without checking its vailidity, but it was added as a
    /// valid identifier to the interner in the first place.
    #[allow(unsafe_code)]
    pub fn resolve_ident(
        &self,
        key: &IdentifierKey,
        key_type: &str,
    ) -> PartialVMResult<Identifier> {
        if let Some(result) = self.0.try_resolve(key) {
            unsafe { Ok(Identifier::new_unchecked(result)) }
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
        self.get_or_intern_str_internal(ident.borrow_str())
    }

    /// Get the interned identifier string value. This may raise an invariant error if
    /// `get_or_intern` fails, but that's likely a serious OOM issue.
    pub fn get_or_intern_ident_str(&self, ident_str: &IdentStr) -> PartialVMResult<IdentifierKey> {
        self.get_or_intern_str_internal(ident_str.borrow_str())
    }

    /// Get the interned string value. This may raise an invariant error if `get_or_intern` fails,
    /// but that's likely a serious OOM issue.
    fn get_or_intern_str_internal(&self, string: &str) -> PartialVMResult<IdentifierKey> {
        match self.0.try_get_or_intern(string) {
            Ok(result) => Ok(result),
            Err(err) => Err(PartialVMError::new(StatusCode::INTERNER_LIMIT_REACHED)
                .with_message(format!("Failed to intern string {string}; error: {err:?}."))),
        }
    }
}
