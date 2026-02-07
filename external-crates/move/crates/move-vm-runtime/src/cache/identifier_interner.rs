// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{shared::constants::IDENTIFIER_INTERNER_SIZE_LIMIT};

use move_core_types::identifier::{IdentStr, Identifier};

use lasso::{Spur, ThreadedRodeo};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// A wrapper around a lasso ThreadedRoade with some niceties to make it easier to use in the VM.
#[derive(Debug)]
pub struct IdentifierInterner(ThreadedRodeo);

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
        let memory_limits = lasso::MemoryLimits::new(IDENTIFIER_INTERNER_SIZE_LIMIT);
        let rodeo = ThreadedRodeo::with_memory_limits(memory_limits);
        Self(rodeo)
    }

    // [SAFETY] The unsafe code is creating an identifier without checking its vailidity, but it
    // was added as a valid identifier to the interner in the first place. If we fail to find the
    // key or the key is not a valid identifier, this code will panic. This is preferred, however,
    // as it indicates a serious runtime issue: interner keys ca no longer be trusted, and we
    // should panic immediately rather than risk a network fork by propagating this issue.
    #[allow(unsafe_code, clippy::panic)]
    /// Resolve an identifier in the interner or produce an invariant violation. This is for use
    /// when the key _must_ be there, as it produces an error when it is not found. The `key_type`
    /// is used to make a more-informative error message.
    pub(crate) fn resolve_ident(&self, key: &IdentifierKey, key_type: &str) -> Identifier {
        if let Some(result) = self.0.try_resolve(&key.0) {
            unsafe { Identifier::new_unchecked(result) }
        } else {
            panic!("Failed to resolve {key_type} key in ident interner: {key:?}")
        }
    }

    /// Get the interned identifier value. This may raise an invariant error if `try_get_or_intern`
    /// fails, but that's likely a serious OOM issue.
    pub(crate) fn intern_identifier(&self, ident: &Identifier) -> IdentifierKey {
        self.get_or_intern_str_internal(ident.borrow_str())
    }

    /// Get the interned identifier string value. This may raise an invariant error if
    /// `get_or_intern` fails, but that's likely a serious OOM issue.
    pub(crate) fn intern_ident_str(&self, ident_str: &IdentStr) -> IdentifierKey {
        self.get_or_intern_str_internal(ident_str.borrow_str())
    }

    #[allow(clippy::panic)]
    // [SAFETY] The unsafe code is inserting an identifier into the interner without ensuring it
    // will fit. If it fails to fit, it will panic, but that's likely a serious OOM issue, and it's
    // better to panic here than to propagate OOM errors throughout the VM and potentially risk a
    // network fork.
    fn get_or_intern_str_internal(&self, string: &str) -> IdentifierKey {
        // Prefer to panic here rather than propagate OOM errors throughout the VM---reporting an
        // invariant violation during an OOM could result in a consensus issue.
        match self.0.try_get_or_intern(string) {
            Ok(result) => IdentifierKey(result),
            Err(err) => panic!("Identifier interner OOM: {err:?}"),
        }
    }

    /// Get the current size of the interner in bytes.
    pub(crate) fn size(&self) -> usize {
        self.0.current_memory_usage()
    }
}
