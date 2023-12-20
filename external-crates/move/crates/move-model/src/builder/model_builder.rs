// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Translates and validates specification language fragments as they are output from the Move
//! compiler's expansion phase and adds them to the environment (which was initialized from the
//! byte code). This includes identifying the Move sub-language supported by the specification
//! system, as well as type checking it and translating it to the spec language ast.

use std::collections::{BTreeMap, BTreeSet};

use num::BigUint;

use move_compiler::{expansion::ast as EA, parser::ast as PA, shared::NumericalAddress};

use crate::{
    ast::{Attribute, ModuleName, QualifiedSymbol, Value},
    model::{FunId, FunctionVisibility, GlobalEnv, Loc, ModuleId, StructId},
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
    /// A symbol table storing unused schemas, used later to generate warnings. All schemas
    /// are initially in the table and are removed when they are used in expressions.
    pub unused_schema_set: BTreeSet<QualifiedSymbol>,
    /// A symbol table for structs.
    pub struct_table: BTreeMap<QualifiedSymbol, StructEntry>,
    /// A reverse mapping from ModuleId/StructId pairs to QualifiedSymbol. This
    /// is used for visualization of types in error messages.
    pub reverse_struct_table: BTreeMap<(ModuleId, StructId), QualifiedSymbol>,
    /// A symbol table for functions.
    pub fun_table: BTreeMap<QualifiedSymbol, FunEntry>,
    /// A symbol table for constants.
    pub const_table: BTreeMap<QualifiedSymbol, ConstEntry>,
}

/// A declaration of a schema in the builders state.
#[derive(Debug)]
pub(crate) struct SpecSchemaEntry {
    pub loc: Loc,
    #[allow(dead_code)]
    pub name: QualifiedSymbol,
    pub module_id: ModuleId,
    pub type_params: Vec<(Symbol, Type)>,
    // The local variables declared in the schema.
    pub vars: Vec<(Symbol, Type)>,
    // All variables in scope of this schema, including those introduced by included schemas.
    pub all_vars: BTreeMap<Symbol, LocalVarEntry>,
}

/// A declaration of a struct.
#[derive(Debug, Clone)]
pub(crate) struct StructEntry {
    pub loc: Loc,
    pub module_id: ModuleId,
    pub struct_id: StructId,
    #[allow(dead_code)]
    pub is_resource: bool,
    pub type_params: Vec<(Symbol, Type)>,
    pub fields: Option<BTreeMap<Symbol, (usize, Type)>>,
    pub attributes: Vec<Attribute>,
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
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone)]
pub(crate) struct ConstEntry {
    pub loc: Loc,
    pub ty: Type,
    pub value: Value,
}

impl<'env> ModelBuilder<'env> {
    /// Creates a builders.
    pub fn new(env: &'env mut GlobalEnv) -> Self {
        let mut translator = ModelBuilder {
            env,
            unused_schema_set: BTreeSet::new(),
            struct_table: BTreeMap::new(),
            reverse_struct_table: BTreeMap::new(),
            fun_table: BTreeMap::new(),
            const_table: BTreeMap::new(),
        };
        translator
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
        struct_id: StructId,
        is_resource: bool,
        type_params: Vec<(Symbol, Type)>,
        fields: Option<BTreeMap<Symbol, (usize, Type)>>,
    ) {
        let entry = StructEntry {
            loc,
            attributes,
            module_id,
            struct_id,
            is_resource,
            type_params,
            fields,
        };
        // Duplicate declarations have been checked by the Move compiler.
        assert!(self.struct_table.insert(name.clone(), entry).is_none());
        self.reverse_struct_table
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
        self.struct_table
            .get(name)
            .cloned()
            .map(|e| {
                Type::Struct(
                    e.module_id,
                    e.struct_id,
                    e.type_params.iter().map(|(_, t)| t).cloned().collect(),
                )
            })
            .unwrap_or_else(|| {
                self.error(
                    loc,
                    &format!("undeclared `{}`", name.display_full(self.env.symbol_pool())),
                );
                Type::Error
            })
    }

    /// Returns the symbol for a binary op.
    pub fn bin_op_symbol(&self, op: &PA::BinOp_) -> QualifiedSymbol {
        QualifiedSymbol {
            module_name: self.builtin_module(),
            symbol: self.env.symbol_pool().make(op.symbol()),
        }
    }

    /// Returns the symbol for a unary op.
    pub fn unary_op_symbol(&self, op: &PA::UnaryOp_) -> QualifiedSymbol {
        QualifiedSymbol {
            module_name: self.builtin_module(),
            symbol: self.env.symbol_pool().make(op.symbol()),
        }
    }

    /// Returns the symbol for a name in the builtin module.
    pub fn builtin_qualified_symbol(&self, name: &str) -> QualifiedSymbol {
        QualifiedSymbol {
            module_name: self.builtin_module(),
            symbol: self.env.symbol_pool().make(name),
        }
    }

    /// Returns the symbol for the builtin function `old`.
    pub fn old_symbol(&self) -> Symbol {
        self.env.symbol_pool().make("old")
    }

    /// Returns the symbol for the builtin Move function `assert`.
    pub fn assert_symbol(&self) -> Symbol {
        self.env.symbol_pool().make("assert")
    }

    /// Returns the name for the pseudo builtin module.
    pub fn builtin_module(&self) -> ModuleName {
        ModuleName::new(BigUint::default(), self.env.symbol_pool().make("$$"))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LocalVarEntry {
    pub loc: Loc,
    pub type_: Type,
    /// If this a temporary from Move code, this is it's index.
    pub temp_index: Option<usize>,
}
