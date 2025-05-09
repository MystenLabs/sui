// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compiler_info::CompilerInfo,
    symbols::{
        cursor::CursorContext,
        def_info::DefInfo,
        mod_defs::ModuleDefs,
        use_def::{References, UseDefMap},
    },
};

use std::{
    cmp,
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use move_compiler::{naming::ast::Type, shared::files::MappedFiles};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;

/// Definition of a local (or parameter)
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LocalDef {
    /// Location of the definition
    pub def_loc: Loc,
    /// Type of definition
    pub def_type: Type,
}

impl PartialOrd for LocalDef {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for LocalDef {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.def_loc.cmp(&other.def_loc)
    }
}

/// Map from struct name to field order information
pub type StructFieldOrderInfo = BTreeMap<Symbol, BTreeMap<Symbol, usize>>;
/// Map from enum name to variant name to field order information
pub type VariantFieldOrderInfo = BTreeMap<Symbol, BTreeMap<Symbol, BTreeMap<Symbol, usize>>>;

/// Information about field order in structs and enums needed for auto-completion
/// to be consistent with field order in the source code
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct FieldOrderInfo {
    pub structs: BTreeMap<String, StructFieldOrderInfo>,
    pub variants: BTreeMap<String, VariantFieldOrderInfo>,
}

impl Default for FieldOrderInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl FieldOrderInfo {
    pub fn new() -> Self {
        Self {
            structs: BTreeMap::new(),
            variants: BTreeMap::new(),
        }
    }
}

pub type DefMap = BTreeMap<Loc, DefInfo>;
pub type FileUseDefs = BTreeMap<PathBuf, UseDefMap>;
pub type FileModules = BTreeMap<PathBuf, BTreeSet<ModuleDefs>>;

/// Result of the symbolication process
#[derive(Debug, Clone)]
pub struct Symbols {
    /// A map from def locations to all the references (uses)
    pub references: References,
    /// A mapping from uses to definitions in a file
    pub file_use_defs: FileUseDefs,
    /// A mapping from filePath to ModuleDefs
    pub file_mods: FileModules,
    /// Mapped file information for translating locations into positions
    pub files: MappedFiles,
    /// Additional information about definitions
    pub def_info: DefMap,
    /// IDE Annotation Information from the Compiler
    pub compiler_info: CompilerInfo,
    /// Cursor information gathered up during analysis
    pub cursor_context: Option<CursorContext>,
}
