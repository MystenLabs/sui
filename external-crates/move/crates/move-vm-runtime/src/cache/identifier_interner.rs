// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    identifier::{IdentStr, Identifier},
    vm_status::StatusCode,
};

use lasso::{Spur, ThreadedRodeo};

use once_cell::sync::Lazy;
use std::sync::Arc;

// -------------------------------------------------------------------------------------------------
// Global Interner
// -------------------------------------------------------------------------------------------------

/// IDENTIFIER INTERNER
/// The Ientifier Interner is global across Move Runtimes and defined here. This is for two reasons:
/// 1. The interner is _always_ a win compared to non-interned identifiers, which hold their
///    strings in boxes. This is always a strict memory win, in all cases. The overall size of the
///    interner plus its definitions is always going to be smaller than holding those individual
///    identifiers.
/// 2. Different runs will benefit from intern reuse: even if the runtime is discarded, interning
///    is a near-constant cost when spinning up a new runtime. Moreover, the interner can be set to
///    have a maximum memory it will refuse to exceed.
///    TODO: Set up this; `lasso` supports it but we need to expose that interface.
/// 3. If absolutely necessary, the execution layer _can_ dump the interner.
static STRING_INTERNER: Lazy<Arc<IdentifierInterner>> =
    Lazy::new(|| Arc::new(IdentifierInterner::new()));

#[cfg(msim)]
pub fn init_interner() {
    let _ = &*STRING_INTERNER;
}

/// Function to access the global StringInterner
fn global_interner() -> Arc<IdentifierInterner> {
    Arc::clone(&STRING_INTERNER)
}

/// Get the interned identifier value. This may raise an invariant error if `try_get_or_intern`
/// fails, but that's likely a serious OOM issue.
pub fn intern_identifier(ident: &Identifier) -> PartialVMResult<IdentifierKey> {
    let interner = global_interner();
    interner.get_or_intern_str_internal(ident.borrow_str())
}

/// Get the interned identifier string value. This may raise an invariant error if
/// `get_or_intern` fails, but that's likely a serious OOM issue.
pub fn intern_ident_str(ident_str: &IdentStr) -> PartialVMResult<IdentifierKey> {
    let interner = global_interner();
    interner.get_or_intern_str_internal(ident_str.borrow_str())
}

/// Get the interned identifier value, using `key_type` in the error case. This may raise an invariant error if `try_get_or_intern`
/// fails, but that's likely a serious OOM issue.
pub fn intern_identifier_with_msg(
    ident: &Identifier,
    key_type: &str,
) -> PartialVMResult<IdentifierKey> {
    let interner = global_interner();
    interner
        .get_or_intern_str_internal(ident.borrow_str())
        .map_err(|err| err.with_message(format!("While attempting to intern {key_type}")))
}

/// Resolve an identifier in the interner or produce an invariant violation. This is for use
/// when the key _must_ be there, as it produces an error when it is not found. The `key_type`
/// is used to make a more-informative error message.
pub fn resolve_interned(key: &IdentifierKey, key_type: &str) -> PartialVMResult<Identifier> {
    let interner = global_interner();
    interner.resolve_ident(key, key_type)
}

/// Get current memory size of the global interner.
pub fn get_interner_size() -> usize {
    let interner = global_interner();
    interner.0.current_memory_usage()
}

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
pub struct IdentifierKey(Spur);

// -------------------------------------------------------------------------------------------------
// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Default for IdentifierInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentifierInterner {
    pub fn new() -> Self {
        let rodeo = ThreadedRodeo::with_capacity(lasso::Capacity::for_strings(IDENTIFIER_SLOTS));
        Self(rodeo)
    }

    // [SAFETY] The unsafe code is creating an identifier without checking its vailidity, but it
    // was added as a valid identifier to the interner in the first place.
    #[allow(unsafe_code)]
    fn resolve_ident(&self, key: &IdentifierKey, key_type: &str) -> PartialVMResult<Identifier> {
        if let Some(result) = self.0.try_resolve(&key.0) {
            unsafe { Ok(Identifier::new_unchecked(result)) }
        } else {
            Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("Failed to find {key_type} key in ident interner.")),
            )
        }
    }

    pub fn get_or_intern_identifier(&self, ident: &Identifier) -> PartialVMResult<IdentifierKey> {
        self.get_or_intern_str_internal(ident.borrow_str())
    }

    pub fn get_or_intern_ident_str(&self, ident_str: &IdentStr) -> PartialVMResult<IdentifierKey> {
        self.get_or_intern_str_internal(ident_str.borrow_str())
    }

    fn get_or_intern_str_internal(&self, string: &str) -> PartialVMResult<IdentifierKey> {
        match self.0.try_get_or_intern(string) {
            Ok(result) => Ok(IdentifierKey(result)),
            Err(err) => Err(PartialVMError::new(StatusCode::INTERNER_LIMIT_REACHED)
                .with_message(format!("Failed to intern {string} ident; error: {err:?}."))),
        }
    }
}
