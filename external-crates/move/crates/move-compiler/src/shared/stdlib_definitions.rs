// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    FullyCompiledProgram,
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
pub const UNIT_TEST_POISON_FUN_NAME: Symbol = symbol!("poison");

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
// Unit Tests
// -----------------------------------------------

pub fn has_unit_test_module(
    pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
    prog: &P::Program,
) -> bool {
    has_module(pre_compiled_lib, prog, UNIT_TEST_MODULE_NAME)
}

pub fn unit_test_poision(loc: Loc) -> P::NameAccessChain {
    name_access_chain(loc, UNIT_TEST_MODULE_NAME, UNIT_TEST_POISON_FUN_NAME)
}

// -----------------------------------------------
// Helpers
// -----------------------------------------------

fn has_module(
    pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
    prog: &P::Program,
    module: Symbol,
) -> bool {
    has_stdlib_module(prog, module)
        || pre_compiled_lib.is_some_and(|p| has_stdlib_module(&p.parser, module))
}

fn has_stdlib_module(prog: &P::Program, module: Symbol) -> bool {
    prog.lib_definitions
        .iter()
        .chain(prog.source_definitions.iter())
        .any(|pkg| match &pkg.def {
            P::Definition::Module(mdef) => {
                mdef.name.0.value == module
                    && mdef.address.is_some()
                    && match &mdef.address.as_ref().unwrap().value {
                        P::LeadingNameAccess_::Name(name) => name.value == STDLIB_ADDRESS_NAME,
                        P::LeadingNameAccess_::GlobalAddress(name) => {
                            name.value == STDLIB_ADDRESS_NAME
                        }
                        P::LeadingNameAccess_::AnonymousAddress(_) => false,
                    }
            }
            _ => false,
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
