// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains code for handling module-leve definitions
//! and other module-related info.

use std::collections::{BTreeMap, BTreeSet};

use lsp_types::Position;

use move_command_line_common::files::FileHash;
use move_compiler::{
    expansion::ast::{ModuleIdent, ModuleIdent_},
    naming::ast::Neighbor,
    parser::ast as P,
    shared::unique_map::UniqueMap,
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;

#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct ModuleDefs {
    /// File where this module is located
    pub fhash: FileHash,
    /// Location where this module is located
    pub name_loc: Loc,
    /// Module name
    pub ident: ModuleIdent_,
    /// Struct definitions
    pub structs: BTreeMap<Symbol, MemberDef>,
    /// Enum definitions
    pub enums: BTreeMap<Symbol, MemberDef>,
    /// Const definitions
    pub constants: BTreeMap<Symbol, MemberDef>,
    /// Function definitions
    pub functions: BTreeMap<Symbol, MemberDef>,
    /// Definitions where the type is not explicitly specified
    /// and should be inserted as an inlay hint
    pub untyped_defs: BTreeSet<Loc>,
    /// Information about calls in this module
    pub call_infos: BTreeMap<Loc, CallInfo>,
    /// Position where auto-imports should be inserted
    pub import_insert_info: Option<AutoImportInsertionInfo>,
    /// Dependencies summary
    pub neighbors: UniqueMap<ModuleIdent, Neighbor>,
}

/// Definition of a module member
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MemberDef {
    pub name_loc: Loc,
    pub info: MemberDefInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemberDefInfo {
    Struct {
        field_defs: Vec<FieldDef>,
        positional: bool,
    },
    Enum {
        variants_info: BTreeMap<Symbol, (Loc, Vec<FieldDef>, /* positional */ bool)>,
    },
    Fun {
        attrs: Vec<String>,
    },
    Const,
}

/// Definition of a struct field
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FieldDef {
    pub name: Symbol,
    pub loc: Loc,
}

/// Information about call sites relevant to the IDE
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct CallInfo {
    /// Is it a dot call?
    pub dot_call: bool,
    /// Locations of arguments
    pub arg_locs: Vec<Loc>,
    /// Definition of function being called (as an Option as its computed after
    /// this struct is created)
    pub def_loc: Option<Loc>,
}

/// Information needed for auto-import insertion. We do our best
/// to make the insertion fit with what's already in the source file.
/// In particular, if uses are already preasent, we insert the new import
/// in the following line keeping the tabulation of the previous import.
/// If no imports are present, we insert the new import before the first
/// module member (or before its doc comment if it exists), pushing
/// this member down but keeping its original tabulation.
#[derive(Debug, Clone, Copy, Ord, PartialOrd, PartialEq, Eq)]
pub struct AutoImportInsertionInfo {
    // Kind of auto-import insertion
    pub kind: AutoImportInsertionKind,
    // Position in file where insertion should start
    pub pos: Position,
    // Tabulation in number of spaces
    pub tabulation: usize,
}

/// Module-level definitions and other module-related info
#[derive(Debug, Clone, Copy, Ord, PartialOrd, PartialEq, Eq)]
pub enum AutoImportInsertionKind {
    AfterLastImport,
    BeforeFirstMember, // when no imports exist
}

//**************************************************************************************************
// Impls
//**************************************************************************************************

impl ModuleDefs {
    pub fn functions(&self) -> &BTreeMap<Symbol, MemberDef> {
        &self.functions
    }

    pub fn structs(&self) -> &BTreeMap<Symbol, MemberDef> {
        &self.structs
    }

    pub fn fhash(&self) -> FileHash {
        self.fhash
    }

    pub fn untyped_defs(&self) -> &BTreeSet<Loc> {
        &self.untyped_defs
    }

    pub fn ident(&self) -> &ModuleIdent_ {
        &self.ident
    }
}

impl CallInfo {
    pub fn new(dot_call: bool, args: &[P::Exp]) -> Self {
        Self {
            dot_call,
            arg_locs: args.iter().map(|e| e.loc).collect(),
            def_loc: None,
        }
    }
}
