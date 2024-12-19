// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use move_binary_format::file_format::{self, SignatureToken, VariantTag};
use move_compiler::{
    self,
    compiled_unit::AnnotatedCompiledUnit,
    expansion::ast::{self as E, ModuleIdent_},
    naming::ast as N,
    shared::{
        files::{FilesSourceText, MappedFiles},
        program_info::{ConstantInfo, FunctionInfo, ModuleInfo, TypingProgramInfo},
        NumericalAddress,
    },
};
use move_core_types::{
    account_address::AccountAddress, annotated_value, language_storage::ModuleId as CoreModuleId,
};
use move_ir_types::{ast as ir, location::Spanned};
use move_symbol_pool::Symbol;

//**************************************************************************************************
// Types
//**************************************************************************************************

pub struct Model {
    files: MappedFiles,
    root_named_address_map: BTreeMap<Symbol, AccountAddress>,
    info: Arc<TypingProgramInfo>,
    // keeping separate in anticipation of compiled model
    compiled_units: BTreeMap<AccountAddress, BTreeMap<Symbol, AnnotatedCompiledUnit>>,
    // TODO package
    packages: BTreeMap<AccountAddress, PackageData>,
    //     compiled_units: BTreeMap<AccountAddress, BTreeMap<Symbol, AnnotatedCompiledUnit>>,
    //     module_deps: BTreeMap<ModuleId, BTreeMap<ModuleId, /* is immediate */ bool>>,
    //     // reverse mapping of module_deps
    //     module_used_by: BTreeMap<ModuleId, BTreeSet<ModuleId>>,
    //     function_immediate_deps: BTreeMap<QualifiedMemberId, BTreeSet<QualifiedMemberId>>,
    //     // reverse mapping of function_immediate_deps
    //     function_called_by: BTreeMap<QualifiedMemberId, BTreeSet<QualifiedMemberId>>,
}

pub trait TModuleId {
    fn module_id(&self) -> ModuleId;
}

pub type ModuleId = (AccountAddress, Symbol);
pub type QualifiedMemberId = (ModuleId, Symbol);

#[derive(Clone, Copy)]
pub struct Package<'a> {
    addr: AccountAddress,
    // TODO name. We likely want the package name from the root package's named address map
    model: &'a Model,
    data: &'a PackageData,
}

#[derive(Clone, Copy)]
pub struct Module<'a> {
    id: ModuleId,
    package: Package<'a>,
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
    data: &'a StructData,
}

#[derive(Clone, Copy)]
pub struct Enum<'a> {
    name: Symbol,
    module: Module<'a>,
    data: &'a EnumData,
}

#[derive(Clone, Copy)]
pub struct Variant<'a> {
    name: Symbol,
    enum_: Enum<'a>,
    data: &'a VariantData,
}

#[derive(Clone, Copy)]
pub struct Function<'a> {
    name: Symbol,
    module: Module<'a>,
    data: &'a FunctionData,
}

#[derive(Clone, Copy)]
pub struct Constant<'a> {
    name: Symbol,
    module: Module<'a>,
    data: &'a ConstantData,
}

//**************************************************************************************************
// API
//**************************************************************************************************

impl Model {
    pub fn new(
        files: FilesSourceText,
        root_named_address_map: BTreeMap<Symbol, AccountAddress>,
        info: Arc<TypingProgramInfo>,
        compiled_units_vec: Vec<AnnotatedCompiledUnit>,
    ) -> anyhow::Result<Self> {
        let mut compiled_units = BTreeMap::new();
        for unit in compiled_units_vec {
            let package_name = unit.package_name();
            let loc = *unit.loc();
            let addr = unit.named_module.address.into_inner();
            let name = unit.named_module.name;
            let package = compiled_units.entry(addr).or_insert_with(BTreeMap::new);
            if let Some(prev) = package.insert(name, unit) {
                anyhow::bail!(
                    "Duplicate module {}::{}. \n\
                    One in package {} in file {}. \n\
                    And one in package {} in file {}",
                    prev.named_module.address,
                    prev.named_module.name,
                    prev.package_name()
                        .as_ref()
                        .map(|s| s.as_str())
                        .unwrap_or("UNKNOWN"),
                    files[&prev.loc().file_hash()].0,
                    package_name
                        .as_ref()
                        .map(|s| s.as_str())
                        .unwrap_or("UNKNOWN"),
                    files[&loc.file_hash()].0,
                );
            }
        }
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
        let mut model = Self {
            files: MappedFiles::new(files),
            root_named_address_map,
            info,
            compiled_units,
            packages,
        };
        model.compute_dependencies();
        model.compute_function_dependencies();
        Ok(model)
    }

    pub fn maybe_package(&self, addr: &AccountAddress) -> Option<Package<'_>> {
        let data = self.packages.get(addr)?;
        Some(Package {
            addr: *addr,
            model: self,
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

    pub fn compiled(&self) -> &'a AnnotatedCompiledUnit {
        &self.model().compiled_units[&self.id.0][&self.id.1]
    }

    pub fn ident(&self) -> &'a E::ModuleIdent {
        &self.data.ident
    }

    pub fn id(&self) -> ModuleId {
        self.id
    }

    pub fn source_path(&self) -> Symbol {
        self.model()
            .files
            .filename(&self.info().defined_loc.file_hash())
    }

    pub fn doc(&self) -> &str {
        todo!()
    }

    pub fn deps(&self) -> &'a BTreeMap<ModuleId, /* is immediate */ bool> {
        &self.data.deps
    }

    pub fn used_by(&self) -> &'a BTreeMap<ModuleId, /* is immediate */ bool> {
        &self.data.used_by
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

    pub fn compiled(&self) -> &'a file_format::StructDefinition {
        self.module
            .compiled()
            .named_module
            .module
            .struct_def_at(self.data.compiled_idx)
    }

    pub fn compiled_idx(&self) -> file_format::StructDefinitionIndex {
        self.data.compiled_idx
    }

    pub fn datatype_handle(&self) -> &'a file_format::DatatypeHandle {
        self.module
            .compiled()
            .named_module
            .module
            .datatype_handle_at(self.compiled().struct_handle)
    }

    pub fn doc(&self) -> &str {
        todo!()
    }

    pub fn field_doc(&self, _field: Symbol) -> &str {
        todo!()
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

    pub fn compiled(&self) -> &file_format::EnumDefinition {
        self.module
            .compiled()
            .named_module
            .module
            .enum_def_at(self.data.compiled_idx)
    }

    pub fn compiled_idx(&self) -> file_format::EnumDefinitionIndex {
        self.data.compiled_idx
    }

    pub fn datatype_handle(&self) -> &'a file_format::DatatypeHandle {
        self.module
            .compiled()
            .named_module
            .module
            .datatype_handle_at(self.compiled().enum_handle)
    }

    pub fn variants(&self) -> impl Iterator<Item = Variant<'a>> + '_ {
        self.data
            .variants
            .keys()
            .map(move |name| self.variant(*name))
    }

    pub fn variant(&self, name: Symbol) -> Variant<'a> {
        let data = self.data.variants.get(&name).unwrap();
        Variant {
            name,
            enum_: *self,
            data,
        }
    }

    pub fn doc(&self) -> &str {
        todo!()
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

    pub fn compiled(&self) -> &'a file_format::VariantDefinition {
        self.module()
            .compiled()
            .named_module
            .module
            .variant_def_at(self.data.enum_idx, self.data.tag)
    }

    pub fn tag(&self) -> VariantTag {
        self.data.tag
    }

    pub fn doc(&self) -> &str {
        todo!()
    }

    pub fn field_doc(&self, _field: Symbol) -> &str {
        todo!()
    }
}

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

    pub fn compiled(&self) -> &'a file_format::FunctionDefinition {
        self.module
            .compiled()
            .named_module
            .module
            .function_def_at(self.data.compiled_idx)
    }

    pub fn compiled_idx(&self) -> file_format::FunctionDefinitionIndex {
        self.data.compiled_idx
    }

    pub fn doc(&self) -> &str {
        todo!()
    }

    pub fn function_handle(&self) -> &'a file_format::FunctionHandle {
        self.module
            .compiled()
            .named_module
            .module
            .function_handle_at(self.compiled().function)
    }

    pub fn compiled_parameters(&self) -> &'a file_format::Signature {
        self.module
            .compiled()
            .named_module
            .module
            .signature_at(self.function_handle().parameters)
    }

    pub fn compiled_return_type(&self) -> &'a file_format::Signature {
        self.module
            .compiled()
            .named_module
            .module
            .signature_at(self.function_handle().return_)
    }

    /// Returns an iterator over the functions  called by this function.
    pub fn calls(&self) -> &'a BTreeSet<QualifiedMemberId> {
        &self.data.calls
    }

    /// Returns an iterator over the functions that call this function.
    pub fn called_by(&self) -> &'a BTreeSet<QualifiedMemberId> {
        &self.data.called_by
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

    pub fn compiled(&self) -> &'a file_format::Constant {
        self.module
            .compiled()
            .named_module
            .module
            .constant_at(self.data.compiled_idx)
    }

    pub fn compiled_idx(&self) -> file_format::ConstantPoolIndex {
        self.data.compiled_idx
    }

    pub fn doc(&self) -> &str {
        todo!()
    }

    /// Returns the value of the constant as a `annotated_move::MoveValue`.
    /// This result will be cached and it will be deserialized only once.
    pub fn value(&self) -> &'a annotated_value::MoveValue {
        self.data.value.get_or_init(|| {
            let compiled = self.compiled();
            let constant_layout = Self::annotated_constant_layout(&compiled.type_);
            annotated_value::MoveValue::simple_deserialize(&compiled.data, &constant_layout)
                .unwrap()
        })
    }

    /// If the constant is a vector<u8>, it will rendered as a UTF8 string.
    /// If it has some other type (or if the data is not a valid UTF8 string),
    /// it will will call display on the `annotated_move::MoveValue`
    pub fn display_value(&self) -> String {
        let compiled = self.compiled();
        if matches!(&compiled.type_, SignatureToken::Vector(x) if x.as_ref() == &SignatureToken::U8)
        {
            if let Some(str) = bcs::from_bytes::<Vec<u8>>(&compiled.data)
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

            ST::Datatype(_)
            | ST::DatatypeInstantiation(_)
            | ST::Reference(_)
            | ST::MutableReference(_)
            | ST::TypeParameter(_) => unreachable!("{ty:?} is not supported in constants"),
        }
    }
}

//**************************************************************************************************
// Traits
//**************************************************************************************************

impl TModuleId for CoreModuleId {
    fn module_id(&self) -> ModuleId {
        (*self.address(), self.name().as_str().into())
    }
}

impl TModuleId for ModuleId {
    fn module_id(&self) -> ModuleId {
        *self
    }
}

impl TModuleId for (&AccountAddress, &Symbol) {
    fn module_id(&self) -> ModuleId {
        (*self.0, *self.1)
    }
}

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

impl<T: TModuleId> TModuleId for &T {
    fn module_id(&self) -> ModuleId {
        T::module_id(*self)
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
    deps: BTreeMap<ModuleId, /* is immediate */ bool>,
    used_by: BTreeMap<ModuleId, /* is immediate */ bool>,
}

pub struct StructData {
    compiled_idx: file_format::StructDefinitionIndex,
}

struct EnumData {
    compiled_idx: file_format::EnumDefinitionIndex,
    variants: BTreeMap<Symbol, VariantData>,
}

struct VariantData {
    enum_idx: file_format::EnumDefinitionIndex,
    tag: VariantTag,
}

struct FunctionData {
    compiled_idx: file_format::FunctionDefinitionIndex,
    calls: BTreeSet<QualifiedMemberId>,
    // reverse mapping of function_immediate_deps
    called_by: BTreeSet<QualifiedMemberId>,
}

struct ConstantData {
    compiled_idx: file_format::ConstantPoolIndex,
    value: OnceCell<annotated_value::MoveValue>,
}

//**************************************************************************************************
// Construction
//**************************************************************************************************

impl Model {
    fn compute_dependencies(&mut self) {
        fn visit(
            compiled_units: &BTreeMap<AccountAddress, BTreeMap<Symbol, AnnotatedCompiledUnit>>,
            acc: &mut BTreeMap<ModuleId, BTreeMap<ModuleId, bool>>,
            id: ModuleId,
            unit: &AnnotatedCompiledUnit,
        ) {
            if acc.contains_key(&id) {
                return;
            }

            let immediate_deps = unit
                .named_module
                .module
                .immediate_dependencies()
                .into_iter()
                .map(|id| (*id.address(), Symbol::from(id.name().as_str())))
                .collect::<Vec<_>>();
            for immediate_dep in &immediate_deps {
                let unit = &compiled_units[&immediate_dep.0][&immediate_dep.1];
                visit(compiled_units, acc, *immediate_dep, unit);
            }
            let mut deps = BTreeMap::new();
            for immediate_dep in immediate_deps {
                deps.insert(immediate_dep, true);
                for transitive_dep in acc.get(&immediate_dep).unwrap().keys() {
                    if !deps.contains_key(transitive_dep) {
                        deps.insert(*transitive_dep, false);
                    }
                }
            }
            acc.insert(id, deps);
        }

        let mut module_deps = BTreeMap::new();
        for (a, units) in &self.compiled_units {
            for (m, unit) in units {
                let id = (*a, *m);
                visit(&self.compiled_units, &mut module_deps, id, unit);
            }
        }
        let mut module_used_by = module_deps
            .keys()
            .map(|id| (*id, BTreeMap::new()))
            .collect::<BTreeMap<_, _>>();
        for (id, deps) in &module_deps {
            for (dep, immediate) in deps {
                let immediate = *immediate;
                let used_by = module_used_by.get_mut(dep).unwrap();
                let is_immediate = used_by.entry(*id).or_insert(false);
                *is_immediate = *is_immediate || immediate;
            }
        }
        for (a, package) in &mut self.packages {
            for (m, data) in &mut package.modules {
                let id = (*a, *m);
                data.deps = module_deps.remove(&id).unwrap();
                data.used_by = module_used_by.remove(&id).unwrap();
            }
        }
    }

    fn compute_function_dependencies(&mut self) {
        let mut function_immediate_deps: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
        let units = self
            .compiled_units
            .iter()
            .flat_map(|(a, units)| units.iter().map(|(m, u)| ((*a, *m), u)));
        for (id, unit) in units {
            let module = &unit.named_module.module;
            for fdef in module.function_defs() {
                let fhandle = module.function_handle_at(fdef.function);
                let fname = module.identifier_at(fhandle.name);
                let qualified_id = (id, Symbol::from(fname.as_str()));
                let callees = fdef
                    .code
                    .as_ref()
                    .iter()
                    .flat_map(|c| c.code.iter())
                    .filter_map(|instr| match instr {
                        file_format::Bytecode::Call(i) => Some(*i),
                        file_format::Bytecode::CallGeneric(i) => {
                            Some(module.function_instantiation_at(*i).handle)
                        }
                        _ => None,
                    })
                    .map(|i| {
                        let callee_handle = module.function_handle_at(i);
                        let callee_module = module
                            .module_id_for_handle(module.module_handle_at(callee_handle.module))
                            .module_id();
                        let callee_name = module.identifier_at(fhandle.name);
                        (callee_module, Symbol::from(callee_name.as_str()))
                    })
                    .collect();
                function_immediate_deps.insert(qualified_id, callees);
            }
        }

        // ensure the map is populated for all functions
        let mut function_called_by = function_immediate_deps
            .values()
            .flatten()
            .map(|callee| (*callee, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        for (caller, callees) in &function_immediate_deps {
            for callee in callees {
                function_called_by.get_mut(callee).unwrap().insert(*caller);
            }
        }
        for (a, package) in &mut self.packages {
            for (m, data) in &mut package.modules {
                let id = (*a, *m);
                for (fname, fdata) in &mut data.functions {
                    let qualified_id = (id, *fname);
                    fdata.calls = function_immediate_deps
                        .remove(&qualified_id)
                        .unwrap_or(BTreeSet::new());
                    fdata.called_by = function_called_by
                        .remove(&qualified_id)
                        .unwrap_or(BTreeSet::new());
                }
            }
        }
    }
}

impl PackageData {
    fn new(
        name: Option<Symbol>,
        addr: AccountAddress,
        ident_map: &BTreeMap<ModuleId, E::ModuleIdent>,
        info: &TypingProgramInfo,
        units: &BTreeMap<Symbol, AnnotatedCompiledUnit>,
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
        unit: &AnnotatedCompiledUnit,
    ) -> Self {
        let structs = info
            .structs
            .iter()
            .map(|(_loc, name, _sinfo)| {
                let name = *name;
                let (idx, _struct_def) = unit
                    .named_module
                    .module
                    .find_struct_def_by_name(name.as_str())
                    .unwrap();
                let struct_ = StructData::new(idx);
                (name, struct_)
            })
            .collect();
        let enums = info
            .enums
            .iter()
            .map(|(_loc, name, _einfo)| {
                let name = *name;
                let (idx, enum_def) = unit
                    .named_module
                    .module
                    .find_enum_def_by_name(name.as_str())
                    .unwrap();
                let enum_ = EnumData::new(&unit.named_module.module, idx, enum_def);
                (name, enum_)
            })
            .collect();
        let functions = info
            .functions
            .iter()
            .map(|(_loc, name, _finfo)| {
                let name = *name;
                let (idx, _function_def) = unit
                    .named_module
                    .module
                    .find_function_def_by_name(name.as_str())
                    .unwrap();
                let function = FunctionData::new(idx);
                (name, function)
            })
            .collect();
        let constants = info
            .constants
            .iter()
            .map(|(_loc, name, _cinfo)| {
                let name = *name;
                let cname = ir::ConstantName(name);
                let idx = *unit
                    .named_module
                    .source_map
                    .constant_map
                    .get(&cname)
                    .unwrap();
                let idx = file_format::ConstantPoolIndex(idx);
                let constant = ConstantData::new(idx);
                (name, constant)
            })
            .collect();
        Self {
            ident,
            structs,
            enums,
            functions,
            constants,
            deps: BTreeMap::new(),
            used_by: BTreeMap::new(),
        }
    }
}

impl StructData {
    fn new(compiled_idx: file_format::StructDefinitionIndex) -> Self {
        Self { compiled_idx }
    }
}

impl EnumData {
    fn new(
        module: &file_format::CompiledModule,
        compiled_idx: file_format::EnumDefinitionIndex,
        def: &file_format::EnumDefinition,
    ) -> Self {
        let mut variants = BTreeMap::new();
        for (tag_idx, variant) in def.variants.iter().enumerate() {
            let tag = tag_idx as u16;
            let name = Symbol::from(module.identifier_at(variant.variant_name).as_str());
            let data = VariantData::new(compiled_idx, tag);
            let prev = variants.insert(name, data);
            assert!(prev.is_none());
        }
        Self {
            compiled_idx,
            variants,
        }
    }
}

impl VariantData {
    fn new(enum_idx: file_format::EnumDefinitionIndex, tag: VariantTag) -> Self {
        Self { enum_idx, tag }
    }
}

impl FunctionData {
    fn new(compiled_idx: file_format::FunctionDefinitionIndex) -> Self {
        Self {
            compiled_idx,
            calls: BTreeSet::new(),
            called_by: BTreeSet::new(),
        }
    }
}

impl ConstantData {
    fn new(compiled_idx: file_format::ConstantPoolIndex) -> Self {
        Self {
            compiled_idx,
            value: OnceCell::new(),
        }
    }
}
