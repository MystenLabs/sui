// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Translates and validates specification language fragments as they are output from the Move
//! compiler's expansion phase and adds them to the environment (which was initialized from the
//! byte code). This includes identifying the Move sub-language supported by the specification
//! system, as well as type checking it and translating it to the spec language ast.

use std::collections::{BTreeMap, BTreeSet};

use codespan_reporting::diagnostic::Severity;
#[allow(unused_imports)]
use log::{debug, info, warn};
use num::BigUint;

use move_compiler::{expansion::ast as EA, parser::ast as PA, shared::NumericalAddress};

use crate::{
    ast::{Attribute, ModuleName, QualifiedSymbol, Value},
    model::{DatatypeId, FunId, FunctionVisibility, GlobalEnv, Loc, ModuleId, QualifiedId},
    project_2nd,
    symbol::Symbol,
    ty::Type,
};

/// A builder is used to enter a sequence of modules in acyclic dependency order into the model. The
/// builder maintains the incremental state of this process, such that the various tables
/// are extended with each module translated. Each table is a mapping from fully qualified names
/// (module names plus item name in the module) to the entity.
#[derive(Debug)]
pub(crate) struct ModelBuilder<'env> {
    /// The global environment we are building.
    pub env: &'env mut GlobalEnv,
    /// A symbol table for datatypes.
    pub datatype_table: BTreeMap<QualifiedSymbol, DatatypeEntry>,
    /// A reverse mapping from ModuleId/DatatypeId pairs to QualifiedSymbol. This
    /// is used for visualization of types in error messages.
    pub reverse_datatype_table: BTreeMap<(ModuleId, DatatypeId), QualifiedSymbol>,
    /// A symbol table for functions.
    pub fun_table: BTreeMap<QualifiedSymbol, FunEntry>,
    /// A symbol table for constants.
    pub const_table: BTreeMap<QualifiedSymbol, ConstEntry>,
}

/// A declaration of a datatype.
#[derive(Debug, Clone)]
pub(crate) struct DatatypeEntry {
    pub loc: Loc,
    pub module_id: ModuleId,
    pub struct_id: DatatypeId,
    pub type_params: Vec<(Symbol, Type)>,
    pub attributes: Vec<Attribute>,
    pub data: DatatypeData,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) enum DatatypeData {
    Struct {
        fields: Option<BTreeMap<Symbol, (usize, Type)>>,
    },
    Enum {
        variants: BTreeMap<Symbol, Option<BTreeMap<Symbol, (usize, Type)>>>,
    },
}

/// A declaration of a function.
#[derive(Debug, Clone)]
pub(crate) struct FunEntry {
    pub loc: Loc,
    pub module_id: ModuleId,
    pub fun_id: FunId,
    pub visibility: FunctionVisibility,
    pub is_entry: bool,
    pub type_params: Vec<(Symbol, Type)>,
    pub params: Vec<(Symbol, Type)>,
    pub result_type: Type,
    pub is_pure: bool,
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone)]
pub(crate) struct ConstEntry {
    pub loc: Loc,
    pub ty: Type,
    pub value: Value,
    pub attributes: Vec<Attribute>,
}

impl<'env> ModelBuilder<'env> {
    /// Creates a builders.
    pub fn new(env: &'env mut GlobalEnv) -> Self {
        ModelBuilder {
            env,
            datatype_table: BTreeMap::new(),
            reverse_datatype_table: BTreeMap::new(),
            fun_table: BTreeMap::new(),
            const_table: BTreeMap::new(),
        }
    }

    /// Shortcut for translating a Move AST location into ours.
    pub fn to_loc(&self, loc: &move_ir_types::location::Loc) -> Loc {
        self.env.to_loc(loc)
    }

    /// Reports a type checking error.
    pub fn error(&self, at: &Loc, msg: &str) {
        self.env.error(at, msg)
    }

    /// Reports a type checking error with notes.
    pub fn error_with_notes(&self, at: &Loc, msg: &str, notes: Vec<String>) {
        self.env.error_with_notes(at, msg, notes)
    }

    /// Defines a struct type.
    pub fn define_struct(
        &mut self,
        loc: Loc,
        attributes: Vec<Attribute>,
        name: QualifiedSymbol,
        module_id: ModuleId,
        struct_id: DatatypeId,
        type_params: Vec<(Symbol, Type)>,
        fields: Option<BTreeMap<Symbol, (usize, Type)>>,
    ) {
        let entry = DatatypeEntry {
            loc,
            attributes,
            module_id,
            struct_id,
            type_params,
            data: DatatypeData::Struct { fields },
        };
        // Duplicate declarations have been checked by the Move compiler.
        assert!(self.datatype_table.insert(name.clone(), entry).is_none());
        self.reverse_datatype_table
            .insert((module_id, struct_id), name);
    }

    pub fn define_enum(
        &mut self,
        loc: Loc,
        attributes: Vec<Attribute>,
        name: QualifiedSymbol,
        module_id: ModuleId,
        struct_id: DatatypeId,
        type_params: Vec<(Symbol, Type)>,
        variants: BTreeMap<Symbol, Option<BTreeMap<Symbol, (usize, Type)>>>,
    ) {
        let entry = DatatypeEntry {
            loc,
            attributes,
            module_id,
            struct_id,
            type_params,
            data: DatatypeData::Enum { variants },
        };
        // Duplicate declarations have been checked by the Move compiler.
        assert!(self.datatype_table.insert(name.clone(), entry).is_none());
        self.reverse_datatype_table
            .insert((module_id, struct_id), name);
    }

    /// Defines a function.
    pub fn define_fun(
        &mut self,
        loc: Loc,
        attributes: Vec<Attribute>,
        name: QualifiedSymbol,
        module_id: ModuleId,
        fun_id: FunId,
        visibility: FunctionVisibility,
        is_entry: bool,
        type_params: Vec<(Symbol, Type)>,
        params: Vec<(Symbol, Type)>,
        result_type: Type,
    ) {
        let entry = FunEntry {
            loc,
            attributes,
            module_id,
            fun_id,
            visibility,
            is_entry,
            type_params,
            params,
            result_type,
            is_pure: false,
        };
        // Duplicate declarations have been checked by the Move compiler.
        assert!(self.fun_table.insert(name, entry).is_none());
    }

    /// Defines a constant.
    pub fn define_const(&mut self, name: QualifiedSymbol, entry: ConstEntry) {
        // Duplicate declarations have been checked by the Move compiler.
        assert!(self.const_table.insert(name, entry).is_none());
    }

    pub fn resolve_address(&self, loc: &Loc, addr: &EA::Address) -> NumericalAddress {
        match addr {
            EA::Address::Numerical { value: bytes, .. } => bytes.value,
            EA::Address::NamedUnassigned(name) => {
                self.error(loc, &format!("Undeclared address `{}`", name));
                NumericalAddress::DEFAULT_ERROR_ADDRESS
            }
        }
    }

    /// Looks up a type (struct), reporting an error if it is not found.
    pub fn lookup_type(&self, loc: &Loc, name: &QualifiedSymbol) -> Type {
        self.datatype_table
            .get(name)
            .cloned()
            .map(|e| Type::Datatype(e.module_id, e.struct_id, project_2nd(&e.type_params)))
            .unwrap_or_else(|| {
                self.error(
                    loc,
                    &format!("undeclared `{}`", name.display_full(self.env.symbol_pool())),
                );
                Type::Error
            })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LocalVarEntry {
    pub loc: Loc,
    pub type_: Type,
    /// If this a temporary from Move code, this is it's index.
    pub temp_index: Option<usize>,
}
