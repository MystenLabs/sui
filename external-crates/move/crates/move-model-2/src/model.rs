// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use move_binary_format::file_format::{self};
use move_compiler::{
    self,
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::FilesSourceText,
    naming::ast as N,
    shared::program_info::{ConstantInfo, FunctionInfo, ModuleInfo, TypingProgramInfo},
    CommentMap,
};
use move_core_types::account_address::AccountAddress;
use move_ir_types::ast as ir;
use move_symbol_pool::Symbol;

//**************************************************************************************************
// Types
//**************************************************************************************************

pub struct ModelData {
    pub(crate) files: Arc<FilesSourceText>,
    pub(crate) comments: CommentMap,
    pub(crate) info: Arc<TypingProgramInfo>,
    pub(crate) compiled_units: BTreeMap<AccountAddress, BTreeMap<Symbol, AnnotatedCompiledUnit>>,
}

pub struct Model<'a> {
    pub(crate) data: &'a ModelData,
    pub(crate) modules: BTreeMap<AccountAddress, BTreeMap<Symbol, Module<'a>>>,
}

pub struct Module<'a> {
    info: &'a ModuleInfo,
    unit: &'a AnnotatedCompiledUnit,
    structs: BTreeMap<Symbol, Struct<'a>>,
    enums: BTreeMap<Symbol, Enum<'a>>,
    functions: BTreeMap<Symbol, Function<'a>>,
    constants: BTreeMap<Symbol, Constant<'a>>,
}

pub struct Struct<'a> {
    info: &'a N::StructDefinition,
    struct_def: &'a file_format::StructDefinition,
}

pub struct Enum<'a> {
    info: &'a N::EnumDefinition,
    // enum_def: &'a file_format::EnumDefinition,
}

pub struct Function<'a> {
    info: &'a FunctionInfo,
    function_def: &'a file_format::FunctionDefinition,
}

pub struct Constant<'a> {
    info: &'a ConstantInfo,
    constant_def: &'a file_format::Constant,
}

//**************************************************************************************************
// API
//**************************************************************************************************

//**************************************************************************************************
// Construction
//**************************************************************************************************

impl<'a> Model<'a> {
    /// Given the data for the model, generate information about the module and its members. Note
    /// that this is O(n) over the modules and should be done once per model.
    pub fn new(data: &'a ModelData) -> Self {
        let modules = data
            .compiled_units
            .iter()
            .map(|(address, units)| {
                let modules = units
                    .iter()
                    .map(|(name, unit)| {
                        let info = data.info.modules.get(&unit.module_ident()).unwrap();
                        let module = Module::new(info, unit);
                        (*name, module)
                    })
                    .collect();
                (*address, modules)
            })
            .collect();
        Self { data, modules }
    }

    pub fn compiled_units(&self) -> impl Iterator<Item = &AnnotatedCompiledUnit> {
        self.data.compiled_units.values().flat_map(|m| m.values())
    }
}

impl<'a> Module<'a> {
    fn new(info: &'a ModuleInfo, unit: &'a AnnotatedCompiledUnit) -> Self {
        let structs = info
            .structs
            .iter()
            .map(|(_loc, name, sinfo)| {
                let name = *name;
                let struct_def = unit
                    .named_module
                    .module
                    .find_struct_def_by_name(name.as_str())
                    .unwrap();
                let struct_ = Struct::new(sinfo, struct_def);
                (name, struct_)
            })
            .collect();
        let functions = info
            .functions
            .iter()
            .map(|(_loc, name, finfo)| {
                let name = *name;
                let function_def = unit
                    .named_module
                    .module
                    .find_function_def_by_name(name.as_str())
                    .unwrap();
                let function = Function::new(finfo, function_def);
                (name, function)
            })
            .collect();
        let constants = info
            .constants
            .iter()
            .map(|(_loc, name, cinfo)| {
                let name = *name;
                let cname = ir::ConstantName(name);
                let constant_idx = *unit
                    .named_module
                    .source_map
                    .constant_map
                    .get(&cname)
                    .unwrap();
                let constant_def = unit
                    .named_module
                    .module
                    .constant_at(file_format::ConstantPoolIndex(constant_idx));
                let constant = Constant::new(cinfo, constant_def);
                (name, constant)
            })
            .collect();

        Self {
            info,
            unit,
            structs,
            enums: BTreeMap::new(),
            functions,
            constants,
        }
    }
}

impl<'a> Struct<'a> {
    fn new(info: &'a N::StructDefinition, struct_def: &'a file_format::StructDefinition) -> Self {
        Self { info, struct_def }
    }
}

impl<'a> Function<'a> {
    fn new(info: &'a FunctionInfo, function_def: &'a file_format::FunctionDefinition) -> Self {
        Self { info, function_def }
    }
}

impl<'a> Constant<'a> {
    fn new(info: &'a ConstantInfo, constant_def: &'a file_format::Constant) -> Self {
        Self { info, constant_def }
    }
}
