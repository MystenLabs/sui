// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use crate::compiled_model::{self, BinaryModel, ModuleId, QualifiedMemberId, TModuleId};
use move_binary_format::file_format;
use move_bytecode_source_map::source_map::SourceMap;
use move_compiler::{
    self,
    compiled_unit::{CompiledUnit, NamedCompiledModule},
    expansion::ast::{self as E, ModuleIdent_},
    naming::ast as N,
    shared::{
        files::MappedFiles,
        program_info::{ConstantInfo, FunctionInfo, ModuleInfo, TypingProgramInfo},
        NumericalAddress,
    },
};
use move_core_types::account_address::AccountAddress;
use move_ir_types::ast as IR;
use move_ir_types::location::Spanned;
use move_symbol_pool::Symbol;

//**************************************************************************************************
// Types
//**************************************************************************************************

pub struct Model {
    files: MappedFiles,
    root_named_address_map: BTreeMap<Symbol, AccountAddress>,
    root_package_name: Option<Symbol>,
    info: Arc<TypingProgramInfo>,
    // compiled_units: BTreeMap<AccountAddress, BTreeMap<Symbol, AnnotatedCompiledUnit>>,
    compiled_model: BinaryModel,
    packages: BTreeMap<AccountAddress, PackageData>,
}

#[derive(Clone, Copy)]
pub struct Package<'a> {
    addr: AccountAddress,
    // TODO name. We likely want the package name from the root package's named address map
    model: &'a Model,
    compiled: &'a compiled_model::Package,
    data: &'a PackageData,
}

#[derive(Clone, Copy)]
pub struct Module<'a> {
    id: ModuleId,
    package: Package<'a>,
    compiled: &'a compiled_model::Module,
    data: &'a ModuleData,
}

#[derive(Clone, Copy)]
pub enum Member<'a> {
    Struct(Struct<'a>),
    Enum(Enum<'a>),
    Function(Function<'a>),
    Constant(Constant<'a>),
}

#[derive(Clone, Copy)]
pub enum Datatype<'a> {
    Struct(Struct<'a>),
    Enum(Enum<'a>),
}

#[derive(Clone, Copy)]
pub struct Struct<'a> {
    name: Symbol,
    module: Module<'a>,
    compiled: &'a compiled_model::Struct,
    #[allow(unused)]
    data: &'a StructData,
}

#[derive(Clone, Copy)]
pub struct Enum<'a> {
    name: Symbol,
    module: Module<'a>,
    compiled: &'a compiled_model::Enum,
    #[allow(unused)]
    data: &'a EnumData,
}

#[derive(Clone, Copy)]
pub struct Variant<'a> {
    name: Symbol,
    enum_: Enum<'a>,
    compiled: &'a compiled_model::Variant,
}

#[derive(Clone, Copy)]
pub struct Function<'a> {
    name: Symbol,
    module: Module<'a>,
    // might be none for macros
    compiled: Option<&'a compiled_model::Function>,
    #[allow(unused)]
    data: &'a FunctionData,
}

#[derive(Clone, Copy)]
pub struct Constant<'a> {
    name: Symbol,
    module: Module<'a>,
    // There is no guarantee a source constant will have a compiled representation
    compiled: Option<&'a compiled_model::Constant>,
    #[allow(unused)]
    data: &'a ConstantData,
}

//**************************************************************************************************
// API
//**************************************************************************************************

impl Model {
    pub fn new(
        files: MappedFiles,
        root_package_name: Option<Symbol>,
        root_named_address_map: BTreeMap<Symbol, AccountAddress>,
        info: Arc<TypingProgramInfo>,
        compiled_units_vec: Vec<(/* file */ PathBuf, CompiledUnit)>,
    ) -> anyhow::Result<Self> {
        let mut compiled_units = BTreeMap::new();
        for (fname, unit) in compiled_units_vec {
            let package_name = unit.package_name();
            let addr = unit.address.into_inner();
            let name = unit.name;
            let package = compiled_units.entry(addr).or_insert_with(BTreeMap::new);
            if let Some((prev_f, prev)) = package.insert(name, (fname.clone(), unit)) {
                anyhow::bail!(
                    "Duplicate module {}::{}. \n\
                    One in package {} in file {}. \n\
                    And one in package {} in file {}",
                    prev.address,
                    prev.name,
                    prev.package_name()
                        .as_ref()
                        .map(|s| s.as_str())
                        .unwrap_or("UNKNOWN"),
                    prev_f.display(),
                    package_name
                        .as_ref()
                        .map(|s| s.as_str())
                        .unwrap_or("UNKNOWN"),
                    fname.display(),
                );
            }
        }
        let compiled_units = compiled_units
            .into_iter()
            .map(|(addr, units)| (addr, units.into_iter().map(|(n, (_f, u))| (n, u)).collect()))
            .collect::<BTreeMap<_, _>>();
        let root_named_address_reverse_map = root_named_address_map
            .iter()
            .map(|(n, a)| (*a, *n))
            .collect::<BTreeMap<_, _>>();
        let ident_map = info
            .modules
            .key_cloned_iter()
            .map(|(ident, _)| (ident.module_id(), ident))
            .collect::<BTreeMap<_, _>>();
        let packages = compiled_units
            .iter()
            .map(|(addr, units)| {
                let name = root_named_address_reverse_map.get(addr).copied();
                let data = PackageData::new(name, *addr, &ident_map, &info, units);
                (*addr, data)
            })
            .collect();
        let compiled_modules = compiled_units
            .into_iter()
            .flat_map(|(_addr, units)| units.into_values().map(|unit| unit.module))
            .collect();
        let compiled_model = BinaryModel::new(compiled_modules);
        let model = Self {
            files,
            root_package_name,
            root_named_address_map,
            info,
            compiled_model,
            packages,
        };
        Ok(model)
    }

    pub fn root_package_name(&self) -> Option<Symbol> {
        self.root_package_name
    }

    pub fn maybe_package(&self, addr: &AccountAddress) -> Option<Package<'_>> {
        let data = self.packages.get(addr)?;
        Some(Package {
            addr: *addr,
            model: self,
            compiled: &self.compiled_model.packages[addr],
            data,
        })
    }
    pub fn package(&self, addr: &AccountAddress) -> Package<'_> {
        self.maybe_package(addr).unwrap()
    }

    /// The name of the package corresponds to the name for the address in the root package's
    /// named address map. This is not the name of the package in the Move.toml file.
    pub fn package_by_name(&self, name: &Symbol) -> Option<Package<'_>> {
        let addr = self.root_named_address_map.get(name)?;
        self.maybe_package(addr)
    }

    pub fn maybe_module(&self, module: impl TModuleId) -> Option<Module<'_>> {
        let (addr, name) = module.module_id();
        let package = self.maybe_package(&addr)?;
        package.maybe_module(name)
    }
    pub fn module(&self, module: impl TModuleId) -> Module<'_> {
        self.maybe_module(module).unwrap()
    }

    pub fn packages(&self) -> impl Iterator<Item = Package<'_>> {
        self.packages.keys().map(|a| self.package(a))
    }

    pub fn modules(&self) -> impl Iterator<Item = Module<'_>> {
        self.packages
            .iter()
            .flat_map(move |(a, p)| p.modules.keys().map(move |m| self.module((a, m))))
    }

    pub fn files(&self) -> &MappedFiles {
        &self.files
    }
}

impl<'a> Package<'a> {
    pub fn address(&self) -> AccountAddress {
        self.addr
    }

    /// The name of the package corresponds to the name for the address in the root package's
    /// named address map. This is not the name of the package in the Move.toml file.
    pub fn name(&self) -> Option<Symbol> {
        self.data.name
    }

    pub fn model(&self) -> &'a Model {
        self.model
    }

    pub fn maybe_module(&self, name: impl Into<Symbol>) -> Option<Module<'a>> {
        let name = name.into();
        let data = self.data.modules.get(&name)?;
        Some(Module {
            id: (self.addr, name),
            package: *self,
            compiled: &self.compiled.modules[&name],
            data,
        })
    }
    pub fn module(&self, name: impl Into<Symbol>) -> Module<'a> {
        self.maybe_module(name).unwrap()
    }

    pub fn modules(&self) -> impl Iterator<Item = Module<'a>> + '_ {
        self.data.modules.keys().map(move |name| self.module(*name))
    }
}

impl<'a> Module<'a> {
    pub fn model(&self) -> &'a Model {
        self.package.model()
    }

    pub fn package(&self) -> Package<'a> {
        self.package
    }

    pub fn maybe_struct(&self, name: impl Into<Symbol>) -> Option<Struct<'a>> {
        let name = name.into();
        let data = &self.data.structs.get(&name)?;
        Some(Struct {
            name,
            module: *self,
            compiled: &self.compiled.structs[&name],
            data,
        })
    }
    pub fn struct_(&self, name: impl Into<Symbol>) -> Struct<'a> {
        self.maybe_struct(name).unwrap()
    }

    pub fn maybe_enum(&self, name: impl Into<Symbol>) -> Option<Enum<'a>> {
        let name = name.into();
        let data = &self.data.enums.get(&name)?;
        Some(Enum {
            name,
            module: *self,
            compiled: &self.compiled.enums[&name],
            data,
        })
    }
    pub fn enum_(&self, name: impl Into<Symbol>) -> Enum<'a> {
        self.maybe_enum(name).unwrap()
    }

    pub fn maybe_function(&self, name: impl Into<Symbol>) -> Option<Function<'a>> {
        let name = name.into();
        let data = &self.data.functions.get(&name)?;
        Some(Function {
            name,
            module: *self,
            compiled: self.compiled.functions.get(&name),
            data,
        })
    }
    pub fn function(&self, name: impl Into<Symbol>) -> Function<'a> {
        self.maybe_function(name).unwrap()
    }

    pub fn maybe_constant(&self, name: impl Into<Symbol>) -> Option<Constant<'a>> {
        let name = name.into();
        let data = &self.data.constants.get(&name)?;
        Some(Constant {
            name,
            module: *self,
            compiled: data
                .compiled_index
                .map(|idx| &self.compiled.constants[idx.0 as usize]),
            data,
        })
    }
    pub fn constant(&self, name: impl Into<Symbol>) -> Constant<'a> {
        self.maybe_constant(name).unwrap()
    }

    pub fn member(&self, name: impl Into<Symbol>) -> Option<Member<'a>> {
        let name = name.into();
        self.maybe_struct(name)
            .map(Member::Struct)
            .or_else(|| self.maybe_enum(name).map(Member::Enum))
            .or_else(|| self.maybe_function(name).map(Member::Function))
            .or_else(|| self.maybe_constant(name).map(Member::Constant))
    }

    pub fn datatype(&self, name: impl Into<Symbol>) -> Option<Datatype<'_>> {
        let name = name.into();
        self.maybe_struct(name)
            .map(Datatype::Struct)
            .or_else(|| self.maybe_enum(name).map(Datatype::Enum))
    }

    pub fn structs(&self) -> impl Iterator<Item = Struct<'a>> + '_ {
        self.data.structs.keys().map(|name| self.struct_(*name))
    }

    pub fn enums(&self) -> impl Iterator<Item = Enum<'a>> + '_ {
        self.data.enums.keys().map(|name| self.enum_(*name))
    }

    pub fn functions(&self) -> impl Iterator<Item = Function<'a>> + '_ {
        self.data.functions.keys().map(|name| self.function(*name))
    }

    pub fn constants(&self) -> impl Iterator<Item = Constant<'a>> + '_ {
        self.data.constants.keys().map(|name| self.constant(*name))
    }

    pub fn info(&self) -> &'a ModuleInfo {
        self.model().info.modules.get(self.ident()).unwrap()
    }

    pub fn compiled(&self) -> &'a compiled_model::Module {
        &self.model().compiled_model.packages[&self.package.addr].modules[&self.name()]
    }

    pub fn ident(&self) -> &'a E::ModuleIdent {
        &self.data.ident
    }

    pub fn name(&self) -> Symbol {
        self.ident().value.module.0.value
    }

    pub fn id(&self) -> ModuleId {
        self.id
    }

    pub fn source_path(&self) -> Symbol {
        self.model()
            .files
            .filename(&self.info().defined_loc.file_hash())
    }

    pub fn deps(&self) -> &'a BTreeMap<ModuleId, /* is immediate */ bool> {
        &self.compiled.deps
    }

    pub fn used_by(&self) -> &'a BTreeMap<ModuleId, /* is immediate */ bool> {
        &self.compiled.used_by
    }
}

impl<'a> Struct<'a> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn model(&self) -> &'a Model {
        self.module.model()
    }

    pub fn package(&self) -> Package<'a> {
        self.module.package()
    }

    pub fn module(&self) -> Module<'a> {
        self.module
    }

    pub fn info(&self) -> &'a N::StructDefinition {
        self.module.info().structs.get_(&self.name).unwrap()
    }

    pub fn compiled(&self) -> &'a compiled_model::Struct {
        self.compiled
    }
}

impl<'a> Enum<'a> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package(&self) -> Package<'a> {
        self.module.package()
    }

    pub fn model(&self) -> &'a Model {
        self.module.model()
    }

    pub fn module(&self) -> Module<'a> {
        self.module
    }

    pub fn info(&self) -> &'a N::EnumDefinition {
        self.module.info().enums.get_(&self.name).unwrap()
    }

    pub fn compiled(&self) -> &'a compiled_model::Enum {
        self.compiled
    }

    pub fn variants(&self) -> impl Iterator<Item = Variant<'a>> + '_ {
        self.compiled
            .variants
            .keys()
            .map(move |name| self.variant(*name))
    }

    pub fn variant(&self, name: Symbol) -> Variant<'a> {
        Variant {
            name,
            enum_: *self,
            compiled: &self.compiled.variants[&name],
        }
    }
}

impl<'a> Variant<'a> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package(&self) -> Package<'a> {
        self.enum_.package()
    }

    pub fn model(&self) -> &'a Model {
        self.enum_.model()
    }

    pub fn module(&self) -> Module<'a> {
        self.enum_.module()
    }

    pub fn enum_(&self) -> Enum<'a> {
        self.enum_
    }

    pub fn info(&self) -> &'a N::VariantDefinition {
        self.enum_.info().variants.get_(&self.name).unwrap()
    }

    pub fn compiled(&self) -> &'a compiled_model::Variant {
        self.compiled
    }
}

static MACRO_EMPTY_SET: LazyLock<&'static BTreeSet<QualifiedMemberId>> =
    LazyLock::new(|| Box::leak(Box::new(BTreeSet::new())));

impl<'a> Function<'a> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package(&self) -> Package<'a> {
        self.module.package()
    }

    pub fn model(&self) -> &'a Model {
        self.module.model()
    }

    pub fn module(&self) -> Module<'a> {
        self.module
    }

    pub fn info(&self) -> &'a FunctionInfo {
        self.module.info().functions.get_(&self.name).unwrap()
    }

    /// Returns the compiled function if it exists. This will be `None` for `macro`s.
    pub fn compiled(&self) -> Option<&'a compiled_model::Function> {
        self.compiled
    }

    /// Returns an the functions called by this function. This will be empty for `macro`s.
    pub fn calls(&self) -> &'a BTreeSet<QualifiedMemberId> {
        match self.compiled {
            Some(f) => &f.calls,
            None => &MACRO_EMPTY_SET,
        }
    }

    /// Returns the functions that call this function. This will be empty for `macro`s.
    pub fn called_by(&self) -> &'a BTreeSet<QualifiedMemberId> {
        match self.compiled {
            Some(f) => &f.called_by,
            None => &MACRO_EMPTY_SET,
        }
    }
}

impl<'a> Constant<'a> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package(&self) -> Package<'a> {
        self.module.package()
    }

    pub fn model(&self) -> &'a Model {
        self.module.model()
    }

    pub fn module(&self) -> Module<'a> {
        self.module
    }

    pub fn info(&self) -> &'a ConstantInfo {
        self.module.info().constants.get_(&self.name).unwrap()
    }

    /// Not all source constants have a compiled representation
    pub fn compiled(&self) -> Option<&'a compiled_model::Constant> {
        self.compiled
    }
}

//**************************************************************************************************
// Traits
//**************************************************************************************************

impl TModuleId for (NumericalAddress, Symbol) {
    fn module_id(&self) -> ModuleId {
        (self.0.into_inner(), self.1)
    }
}

impl TModuleId for (&NumericalAddress, &Symbol) {
    fn module_id(&self) -> ModuleId {
        (self.0.into_inner(), *self.1)
    }
}
impl TModuleId for ModuleIdent_ {
    fn module_id(&self) -> ModuleId {
        let address = self.address.into_addr_bytes().into_inner();
        let module = self.module.0.value;
        (address, module)
    }
}

impl<T: TModuleId> TModuleId for Spanned<T> {
    fn module_id(&self) -> ModuleId {
        T::module_id(&self.value)
    }
}

//**************************************************************************************************
// Internals
//**************************************************************************************************

// The *Data structs are not used currently, but if we need extra source information these provide
// a place to store it.
struct PackageData {
    // Based on the root packages named address map
    name: Option<Symbol>,
    modules: BTreeMap<Symbol, ModuleData>,
}

struct ModuleData {
    ident: E::ModuleIdent,
    structs: BTreeMap<Symbol, StructData>,
    enums: BTreeMap<Symbol, EnumData>,
    functions: BTreeMap<Symbol, FunctionData>,
    constants: BTreeMap<Symbol, ConstantData>,
}

struct StructData {}

struct EnumData {
    #[allow(unused)]
    variants: BTreeMap<Symbol, VariantData>,
}

struct VariantData {}

struct FunctionData {}

struct ConstantData {
    compiled_index: Option<file_format::ConstantPoolIndex>,
}

//**************************************************************************************************
// Construction
//**************************************************************************************************

impl PackageData {
    fn new(
        name: Option<Symbol>,
        addr: AccountAddress,
        ident_map: &BTreeMap<ModuleId, E::ModuleIdent>,
        info: &TypingProgramInfo,
        units: &BTreeMap<Symbol, CompiledUnit>,
    ) -> Self {
        let modules = units
            .iter()
            .map(|(name, unit)| {
                let id = (addr, *name);
                let ident = ident_map.get(&id).unwrap();
                let info = info.module(ident);
                let data = ModuleData::new(id, *ident, info, unit);
                (*name, data)
            })
            .collect();
        Self { name, modules }
    }
}

impl ModuleData {
    fn new(
        _id: ModuleId,
        ident: E::ModuleIdent,
        info: &ModuleInfo,
        unit: &NamedCompiledModule,
    ) -> Self {
        let structs = info
            .structs
            .iter()
            .map(|(_loc, name, _sinfo)| {
                let name = *name;
                let (_idx, _struct_def) =
                    unit.module.find_struct_def_by_name(name.as_str()).unwrap();
                let struct_ = StructData::new();
                (name, struct_)
            })
            .collect();
        let enums = info
            .enums
            .iter()
            .map(|(_loc, name, _einfo)| {
                let name = *name;
                let (_idx, enum_def) = unit.module.find_enum_def_by_name(name.as_str()).unwrap();
                let enum_ = EnumData::new(&unit.module, enum_def);
                (name, enum_)
            })
            .collect();
        let functions = info
            .functions
            .iter()
            .map(|(_loc, name, _finfo)| {
                let name = *name;
                // Note, won't be found for macros
                // let (_idx, _function_def) = unit
                //     .module
                //     .find_function_def_by_name(name.as_str())
                //     .expect(&format!("cannot find fun {name}"));
                let function = FunctionData::new();
                (name, function)
            })
            .collect();
        let constants = info
            .constants
            .iter()
            .map(|(_loc, name, _cinfo)| {
                let name = *name;
                let constant = ConstantData::new(&unit.source_map, name);
                (name, constant)
            })
            .collect();
        Self {
            ident,
            structs,
            enums,
            functions,
            constants,
        }
    }
}

impl StructData {
    fn new() -> Self {
        Self {}
    }
}

impl EnumData {
    fn new(module: &file_format::CompiledModule, def: &file_format::EnumDefinition) -> Self {
        let mut variants = BTreeMap::new();
        for variant in &def.variants {
            let name = Symbol::from(module.identifier_at(variant.variant_name).as_str());
            let data = VariantData::new();
            let prev = variants.insert(name, data);
            assert!(prev.is_none());
        }
        Self { variants }
    }
}

impl VariantData {
    fn new() -> Self {
        Self {}
    }
}

impl FunctionData {
    fn new() -> Self {
        Self {}
    }
}

impl ConstantData {
    fn new(source_map: &SourceMap, name: Symbol) -> Self {
        let compiled_index = source_map
            .constant_map
            .get(&IR::ConstantName(name))
            .copied()
            .map(file_format::ConstantPoolIndex);
        Self { compiled_index }
    }
}
