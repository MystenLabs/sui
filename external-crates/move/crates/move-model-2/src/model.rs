// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{cell::OnceCell, collections::BTreeMap, sync::Arc};

use move_binary_format::file_format::{self, CompiledModule, SignatureToken};
use move_compiler::{
    self,
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::FilesSourceText,
    expansion::ast::ModuleIdent_,
    naming::ast as N,
    shared::{
        program_info::{ConstantInfo, FunctionInfo, ModuleInfo, TypingProgramInfo},
        NumericalAddress,
    },
    CommentMap,
};
use move_core_types::{
    account_address::AccountAddress, annotated_value, language_storage::ModuleId,
};
use move_ir_types::{ast as ir, location::Spanned};
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

pub trait TModuleId {
    fn module_id(&self) -> (AccountAddress, Symbol);
}

pub struct Module<'a> {
    info: &'a ModuleInfo,
    compiled: &'a AnnotatedCompiledUnit,
    structs: BTreeMap<Symbol, Struct<'a>>,
    enums: BTreeMap<Symbol, Enum<'a>>,
    functions: BTreeMap<Symbol, Function<'a>>,
    constants: BTreeMap<Symbol, Constant<'a>>,
}

pub struct Struct<'a> {
    module: &'a CompiledModule,
    info: &'a N::StructDefinition,
    compiled: &'a file_format::StructDefinition,
}

pub struct Enum<'a> {
    module: &'a CompiledModule,
    info: &'a N::EnumDefinition,
    // enum_def: &'a file_format::EnumDefinition,
}

pub struct Function<'a> {
    module: &'a CompiledModule,

    info: &'a FunctionInfo,
    compiled: &'a file_format::FunctionDefinition,
}

pub struct Constant<'a> {
    module: &'a CompiledModule,
    info: &'a ConstantInfo,
    compiled: &'a file_format::Constant,
    value: OnceCell<annotated_value::MoveValue>,
}

//**************************************************************************************************
// API
//**************************************************************************************************

impl<'a> Model<'a> {
    pub fn module(&self, module: impl TModuleId) -> Option<&Module> {
        let (address, name) = module.module_id();
        self.modules.get(&address)?.get(&name)
    }
}

impl<'a> Module<'a> {
    pub fn struct_(&self, name: impl Into<Symbol>) -> Option<&Struct> {
        self.structs.get(&name.into())
    }

    pub fn enum_(&self, name: impl Into<Symbol>) -> Option<&Enum> {
        self.enums.get(&name.into())
    }

    pub fn function(&self, name: impl Into<Symbol>) -> Option<&Function> {
        self.functions.get(&name.into())
    }

    pub fn constant(&self, name: impl Into<Symbol>) -> Option<&Constant> {
        self.constants.get(&name.into())
    }

    pub fn info(&self) -> &ModuleInfo {
        self.info
    }

    pub fn compiled(&self) -> &AnnotatedCompiledUnit {
        self.compiled
    }
}

impl<'a> Struct<'a> {
    pub fn module(&self) -> &CompiledModule {
        self.module
    }

    pub fn info(&self) -> &N::StructDefinition {
        self.info
    }

    pub fn compiled(&self) -> &file_format::StructDefinition {
        self.compiled
    }

    pub fn struct_handle(&self) -> &file_format::StructHandle {
        self.module.struct_handle_at(self.compiled.struct_handle)
    }
}

impl<'a> Enum<'a> {
    pub fn module(&self) -> &CompiledModule {
        self.module
    }

    pub fn info(&self) -> &N::EnumDefinition {
        self.info
    }

    // pub fn compiled(&self) -> &file_format::EnumDefinition {
    //     self.compiled
    // }
}

impl<'a> Function<'a> {
    pub fn module(&self) -> &CompiledModule {
        self.module
    }

    pub fn info(&self) -> &FunctionInfo {
        self.info
    }

    pub fn compiled(&self) -> &file_format::FunctionDefinition {
        self.compiled
    }

    pub fn function_handle(&self) -> &file_format::FunctionHandle {
        self.module.function_handle_at(self.compiled.function)
    }

    pub fn compiled_parameters(&self) -> &file_format::Signature {
        self.module.signature_at(self.function_handle().parameters)
    }

    pub fn compiled_return_type(&self) -> &file_format::Signature {
        self.module.signature_at(self.function_handle().return_)
    }
}

impl<'a> Constant<'a> {
    pub fn module(&self) -> &CompiledModule {
        self.module
    }

    pub fn info(&self) -> &ConstantInfo {
        self.info
    }

    pub fn compiled(&self) -> &file_format::Constant {
        self.compiled
    }

    /// Returns the value of the constant as a `annotated_move::MoveValue`.
    /// This result will be cached and it will be deserialized only once.
    pub fn value(&self) -> &annotated_value::MoveValue {
        self.value.get_or_init(|| {
            let constant_layout = Self::annotated_constant_layout(&self.compiled.type_);
            annotated_value::MoveValue::simple_deserialize(&self.compiled.data, &constant_layout)
                .unwrap()
        })
    }

    /// If the constant is a vector<u8>, it will rendered as a UTF8 string.
    /// If it has some other type (or if the data is not a valid UTF8 string),
    /// it will will call display on the `annotated_move::MoveValue`
    pub fn display_value(&self) -> String {
        if matches!(&self.compiled.type_, SignatureToken::Vector(x) if x.as_ref() == &SignatureToken::U8)
        {
            if let Some(str) = bcs::from_bytes::<Vec<u8>>(&self.compiled.data)
                .ok()
                .and_then(|data| String::from_utf8(data).ok())
            {
                return format!("\"{str}\"");
            }
        }

        format!("{}", self.value())
    }

    fn annotated_constant_layout(ty: &SignatureToken) -> annotated_value::MoveTypeLayout {
        use annotated_value::MoveTypeLayout as L;
        use SignatureToken as ST;
        match ty {
            ST::Bool => L::Bool,
            ST::U8 => L::U8,
            ST::U16 => L::U16,
            ST::U32 => L::U16,
            ST::U64 => L::U64,
            ST::U128 => L::U128,
            ST::U256 => L::U16,
            ST::Address => L::Address,
            ST::Signer => L::Signer,
            ST::Vector(inner) => L::Vector(Box::new(Self::annotated_constant_layout(inner))),

            ST::Struct(_)
            | ST::StructInstantiation(_)
            | ST::Reference(_)
            | ST::MutableReference(_)
            | ST::TypeParameter(_) => unreachable!("{ty:?} is not supported in constants"),
        }
    }
}

//**************************************************************************************************
// Traits
//**************************************************************************************************

impl TModuleId for ModuleId {
    fn module_id(&self) -> (AccountAddress, Symbol) {
        (*self.address(), self.name().as_str().into())
    }
}

impl TModuleId for (AccountAddress, Symbol) {
    fn module_id(&self) -> (AccountAddress, Symbol) {
        *self
    }
}

impl TModuleId for (NumericalAddress, Symbol) {
    fn module_id(&self) -> (AccountAddress, Symbol) {
        (self.0.into_inner(), self.1)
    }
}

impl TModuleId for ModuleIdent_ {
    fn module_id(&self) -> (AccountAddress, Symbol) {
        let address = self.address.into_addr_bytes().into_inner();
        let module = self.module.0.value;
        (address, module)
    }
}

impl<T: TModuleId> TModuleId for &T {
    fn module_id(&self) -> (AccountAddress, Symbol) {
        T::module_id(*self)
    }
}

impl<T: TModuleId> TModuleId for Spanned<T> {
    fn module_id(&self) -> (AccountAddress, Symbol) {
        T::module_id(&self.value)
    }
}

//**************************************************************************************************
// Construction
//**************************************************************************************************

impl ModelData {
    /// Note that this is O(n) over the modules and should be done once per model.
    pub fn model(&self) -> Model<'_> {
        Model::new(self)
    }
}

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
        let module = &unit.named_module.module;
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
                let struct_ = Struct::new(module, sinfo, struct_def);
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
                let function = Function::new(module, finfo, function_def);
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
                let constant = Constant::new(module, cinfo, constant_def);
                (name, constant)
            })
            .collect();

        Self {
            info,
            compiled: unit,
            structs,
            enums: BTreeMap::new(),
            functions,
            constants,
        }
    }
}

impl<'a> Struct<'a> {
    fn new(
        module: &'a CompiledModule,
        info: &'a N::StructDefinition,
        compiled: &'a file_format::StructDefinition,
    ) -> Self {
        Self {
            module,
            info,
            compiled,
        }
    }
}

impl<'a> Function<'a> {
    fn new(
        module: &'a CompiledModule,
        info: &'a FunctionInfo,
        compiled: &'a file_format::FunctionDefinition,
    ) -> Self {
        Self {
            module,
            info,
            compiled,
        }
    }
}

impl<'a> Constant<'a> {
    fn new(
        module: &'a CompiledModule,
        info: &'a ConstantInfo,
        compiled: &'a file_format::Constant,
    ) -> Self {
        Self {
            module,
            info,
            compiled,
            value: OnceCell::new(),
        }
    }
}
