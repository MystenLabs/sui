// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    PreCompiledProgramInfo,
    expansion::ast as E,
    parser::ast::{self as P, NameAccessChain},
};

use move_ir_types::location::{Loc, sp};
use move_symbol_pool::Symbol;

use std::sync::Arc;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StdlibName(Symbol, Symbol);

// -------------------------------------------------------------------------------------------------
// Constants
// -------------------------------------------------------------------------------------------------

pub const STDLIB_ADDRESS_NAME: Symbol = symbol!("std");

// -----------------------------------------------
// Ascii

const ASCII_MODULE_NAME: Symbol = symbol!("ascii");
const ASCII_STRING_CTOR_NAME: Symbol = symbol!("string");
const ASCII_STRING_TYPE_NAME: Symbol = symbol!("String");

pub const ASCII_STRING_CTOR: StdlibName = StdlibName(ASCII_MODULE_NAME, ASCII_STRING_CTOR_NAME);
pub const ASCII_STRING_TYPE: StdlibName = StdlibName(ASCII_MODULE_NAME, ASCII_STRING_TYPE_NAME);

// -----------------------------------------------
// String

const STRING_MODULE_NAME: Symbol = symbol!("string");
const STRING_STRING_CTOR_NAME: Symbol = symbol!("utf8");
const STRING_STRING_TYPE_NAME: Symbol = symbol!("String");

pub const STRING_STRING_CTOR: StdlibName = StdlibName(STRING_MODULE_NAME, STRING_STRING_CTOR_NAME);
pub const STRING_STRING_TYPE: StdlibName = StdlibName(STRING_MODULE_NAME, STRING_STRING_TYPE_NAME);

// -----------------------------------------------
// Unit Tests

pub const UNIT_TEST_MODULE_NAME: Symbol = symbol!("unit_test");
pub const UNIT_TEST_POISON_NATIVE_NAME: Symbol = symbol!("poison");
pub const UNIT_TEST_POISON_INJECTION_NAME: Symbol = symbol!("unit_test_poison");

// -----------------------------------------------
// Std Lib Defintions

pub const STDLIB_CTOR_DEFINITIONS: [(StdlibName, Symbol, Symbol); 2] = [
    (ASCII_STRING_CTOR, ASCII_MODULE_NAME, ASCII_STRING_CTOR_NAME),
    (
        STRING_STRING_CTOR,
        STRING_MODULE_NAME,
        STRING_STRING_CTOR_NAME,
    ),
];

pub const STDLIB_TYPE_DEFINITIONS: [(StdlibName, Symbol, Symbol); 2] = [
    (ASCII_STRING_TYPE, ASCII_MODULE_NAME, ASCII_STRING_TYPE_NAME),
    (
        STRING_STRING_TYPE,
        STRING_MODULE_NAME,
        STRING_STRING_TYPE_NAME,
    ),
];

pub const STDLIB_STRING_TYPES: [(Symbol, Symbol, Symbol); 2] = [
    (
        STDLIB_ADDRESS_NAME,
        ASCII_MODULE_NAME,
        ASCII_STRING_TYPE_NAME,
    ),
    (
        STDLIB_ADDRESS_NAME,
        STRING_MODULE_NAME,
        STRING_STRING_TYPE_NAME,
    ),
];

pub const ASCII_STRING_VALIDATOR: fn(&E::Value_) -> Result<(), String> = is_ascii_string;
pub const STRING_STRING_VALIDATOR: fn(&E::Value_) -> Result<(), String> = is_utf8_string;

// -------------------------------------------------------------------------------------------------
// Functions
// -------------------------------------------------------------------------------------------------

/// Returns a vector of tuples of the qualified name and the name access chain, using the provided
/// location
pub fn stdlib_function_definition(loc: Loc) -> Vec<(StdlibName, NameAccessChain)> {
    STDLIB_CTOR_DEFINITIONS
        .iter()
        .map(|(qualified, module, name)| (*qualified, name_access_chain(loc, *module, *name)))
        .collect::<Vec<_>>()
}

/// Returns a vector of tuples of the qualified name and the name access chain, using the provided
/// location
pub fn stdlib_type_definition(loc: Loc) -> Vec<(StdlibName, NameAccessChain)> {
    STDLIB_TYPE_DEFINITIONS
        .iter()
        .map(|(qualified, module, name)| (*qualified, name_access_chain(loc, *module, *name)))
        .collect::<Vec<_>>()
}

// -----------------------------------------------
// String Utilities
// -----------------------------------------------

/// Indicates if the data is a valid ascii string
pub fn is_ascii_string(value: &E::Value_) -> Result<(), String> {
    use E::Value_ as V;
    // Checks that all bytes in the provided data slice are valid ASCII characters (0x00–0x7F). To
    // do this, we scan through the input byte slice and ensures every byte is within the ASCII
    // range. If we find an invalid (non-ASCII) byte, we build an error including:
    // - Up to three ASCII characters immediately preceding the invalid byte (prepended with "..."
    //   if there are earlier characters omitted).
    // - The invalid byte itself, formatted as a hexadecimal escape (`\xHH`).
    // - Up to three subsequent ASCII characters following the invalid byte (appended with "..." if
    //   additional unseen characters remain).
    fn ensure_ascii(data: &[u8]) -> Result<(), String> {
        if let Some((i, &b)) = data.iter().enumerate().find(|&(_, &b)| !b.is_ascii()) {
            // ----- leading (up to 3 ASCII chars before i) -----
            let lead_start = i.saturating_sub(3);
            let mut leading = std::str::from_utf8(&data[lead_start..i])
                .unwrap()
                .to_string();
            if lead_start > 0 {
                leading.insert_str(0, "...");
            }

            // ----- offending (single non-ASCII byte) -----
            let offending = format!("\\x{:02X}", b);

            // ----- trailing (next up to 3 ASCII chars *after* the invalid byte, skipping non-ASCII) -----
            let mut trail_bytes = Vec::with_capacity(3);
            let mut pos = i + 1;
            while pos < data.len() && trail_bytes.len() < 3 {
                let nb = data[pos];
                if nb.is_ascii() {
                    trail_bytes.push(nb);
                }
                pos += 1;
            }
            let mut trailing = String::new();
            if !trail_bytes.is_empty() {
                trailing = std::str::from_utf8(&trail_bytes).unwrap().to_string();
            }
            if pos < data.len() {
                // There are still bytes (ASCII or not) after what we showed
                trailing.push_str("...");
            }

            return Err(format!(
                "string \"{}{}{}\". contains invalid ASCII entry at char '{}' at index '{}'",
                leading, offending, trailing, offending, i
            ));
        }
        Ok(())
    }

    match value {
        V::Address(_)
        | V::InferredNum(_)
        | V::U8(_)
        | V::U16(_)
        | V::U32(_)
        | V::U64(_)
        | V::U128(_)
        | V::U256(_)
        | V::Bool(_)
        | V::Bytearray(_) => Err("value is not a string".to_owned()),
        V::InferredString(data) => ensure_ascii(data),
    }
}

/// Indicates if the data is a valid UTF8 string
pub fn is_utf8_string(value: &E::Value_) -> Result<(), String> {
    use E::Value_ as V;

    // Checks that the provided data slice contains only valid UTF-8 encoded Unicode characters. To
    // do this, we attempt to decode the entire byte slice as UTF-8. If we encounter an invalid
    // UTF-8 byte sequence, we construct an error message including:
    // - Up to three Unicode characters immediately before the invalid sequence (prepended with
    //   "..." if there are earlier characters omitted).
    // - The offending bytes themselves, formatted as hexadecimal escapes (`\xHH\xMM...`).
    // - Up to three Unicode characters immediately after the invalid sequence (appended with
    //   "..." if additional unseen characters remain).
    fn ensure_unicode(data: &[u8]) -> Result<(), String> {
        use std::fmt::Write;
        match std::str::from_utf8(data) {
            Ok(_) => Ok(()),
            Err(e) => {
                let i = e.valid_up_to();
                let remaining = data.len().saturating_sub(i);
                let seq_len = e.error_len().unwrap_or(remaining);
                let end = i + seq_len.min(remaining);

                // offending bytes as "\xNN\xMM..."
                let offending =
                    data[i..end]
                        .iter()
                        .fold(String::with_capacity((end - i) * 4), |mut s, &b| {
                            let _ = write!(s, "\\x{:02X}", b);
                            s
                        });

                // ----- leading (last up to 3 Unicode chars before i) -----
                // Safe: up to `i` is valid UTF-8.
                let prefix = unsafe { std::str::from_utf8_unchecked(&data[..i]) };
                let lead_start = prefix
                    .char_indices()
                    .rev()
                    .nth(2) // 3rd-from-end -> start of last 3 chars
                    .map(|(p, _)| p)
                    .unwrap_or(0);
                let mut leading = prefix[lead_start..].to_string();
                if lead_start > 0 {
                    leading.insert_str(0, "...");
                }

                // ----- trailing (first up to 3 Unicode chars after the invalid seq) -----
                let trailing_start = end;
                let mut trailing = String::new();
                let mut consumed_bytes = 0usize;

                if trailing_start < data.len() {
                    if let Ok(suffix) = std::str::from_utf8(&data[trailing_start..]) {
                        for (j, ch) in suffix.char_indices() {
                            if trailing.chars().count() >= 3 {
                                break;
                            }
                            trailing.push(ch);
                            consumed_bytes = j + ch.len_utf8();
                        }
                        // ellipsis if there are more bytes after what we showed
                        if trailing_start + consumed_bytes < data.len() {
                            trailing.push_str("...");
                        }
                    } else {
                        // can't decode any trailing chars; still show ellipsis if bytes remain
                        trailing.push_str("...");
                    }
                }

                Err(format!(
                    "string \"{}{}{}\". contains invalid UTF-8 entry at bytes '{}' at index '{}'",
                    leading, offending, trailing, offending, i
                ))
            }
        }
    }

    match value {
        V::Address(_)
        | V::InferredNum(_)
        | V::U8(_)
        | V::U16(_)
        | V::U32(_)
        | V::U64(_)
        | V::U128(_)
        | V::U256(_)
        | V::Bool(_)
        | V::Bytearray(_) => Err("value is not a string".to_owned()),
        V::InferredString(data) => ensure_unicode(data),
    }
}

// -----------------------------------------------
// Unit Tests
// -----------------------------------------------

pub fn has_unit_test_module(
    prog: &P::Program,
    pre_compiled_lib: Option<Arc<PreCompiledProgramInfo>>,
) -> bool {
    has_stdlib_module(prog, pre_compiled_lib, UNIT_TEST_MODULE_NAME)
}

pub fn unit_test_poision_native(loc: Loc) -> P::NameAccessChain {
    name_access_chain(loc, UNIT_TEST_MODULE_NAME, UNIT_TEST_POISON_NATIVE_NAME)
}

// -----------------------------------------------
// Helpers
// -----------------------------------------------

fn has_stdlib_module(
    prog: &P::Program,
    pre_compiled_lib: Option<Arc<PreCompiledProgramInfo>>,
    module: Symbol,
) -> bool {
    fn stdlib_addr_name(sp!(_, n): &P::LeadingNameAccess) -> bool {
        match n {
            P::LeadingNameAccess_::Name(name) => name.value == STDLIB_ADDRESS_NAME,
            P::LeadingNameAccess_::GlobalAddress(name) => name.value == STDLIB_ADDRESS_NAME,
            P::LeadingNameAccess_::AnonymousAddress(_) => false,
        }
    }

    prog.lib_definitions
        .iter()
        .chain(prog.source_definitions.iter())
        .any(|pkg| {
            let P::Definition::Module(mdef) = &pkg.def else {
                return false;
            };
            mdef.name.0.value == module
                && mdef.address.is_some()
                && stdlib_addr_name(mdef.address.as_ref().unwrap())
        })
        || pre_compiled_lib.is_some_and(|mdefs| {
            mdefs
                .iter()
                .any(|(sp!(_, mident), _)| mident.named_address_is(STDLIB_ADDRESS_NAME, module))
        })
}

fn name_access_chain(loc: Loc, mod_: Symbol, name: Symbol) -> P::NameAccessChain {
    let path = P::NamePath {
        root: P::RootPathEntry {
            name: stdlib_address_name(loc),
            tyargs: None,
            is_macro: None,
        },
        entries: vec![
            P::PathEntry {
                name: sp(loc, mod_),
                tyargs: None,
                is_macro: None,
            },
            P::PathEntry {
                name: sp(loc, name),
                tyargs: None,
                is_macro: None,
            },
        ],
        is_incomplete: false,
    };
    sp(loc, P::NameAccessChain_::Path(path))
}

fn stdlib_address_name(loc: Loc) -> P::LeadingNameAccess {
    sp(
        loc,
        P::LeadingNameAccess_::Name(sp(loc, STDLIB_ADDRESS_NAME)),
    )
}

// -----------------------------------------------
// Other Impls
// -----------------------------------------------

impl std::fmt::Display for StdlibName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.0, self.1)
    }
}
