// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    normalized::{self, ModuleId, QualifiedMemberId, TModuleId},
    source_kind::{AnyKind, SourceKind, Uninit, WithSource, WithoutSource},
    source_model, summary,
};
use indexmap::IndexMap;
use move_binary_format::file_format;
use move_bytecode_source_map::source_map::SourceMap;
use move_compiler::{
    self,
    compiled_unit::NamedCompiledModule,
    expansion::ast::{self as E, ModuleIdent_},
    shared::{
        NumericalAddress,
        files::MappedFiles,
        program_info::{ModuleInfo, TypingProgramInfo},
    },
};
use move_core_types::{account_address::AccountAddress, runtime_value};
use move_ir_types::ast as IR;
use move_ir_types::location::Spanned;
use move_symbol_pool::Symbol;

//**************************************************************************************************
// Types
//**************************************************************************************************

#[derive(Clone, Copy)]
pub enum Kind<TWithSource, TWithout> {
    WithSource(TWithSource),
    WithoutSource(TWithout),
}

/// The model for a set of packages. Allows for ergonomic access to packages, modules, and
/// module members. If source files are present, the Move package system can be used to generate
/// a `Model<WithSource>` via `Model::from_source`. If no source files are present, a model can be
/// generated directly from the `CompiledModule`s via `Model::from_compiled`.
pub struct Model<K: SourceKind> {
    pub(crate) has_source: bool,
    pub(crate) files: K::FromSource<MappedFiles>,
    pub(crate) root_named_address_map: BTreeMap<Symbol, AccountAddress>,
    pub(crate) root_named_address_reverse_map: BTreeMap<AccountAddress, Symbol>,
    pub(crate) root_package_name: Option<Symbol>,
    pub(crate) info: K::FromSource<Arc<TypingProgramInfo>>,
    pub(crate) compiled: normalized::Packages,
    pub(crate) packages: BTreeMap<AccountAddress, PackageData<K>>,
    pub(crate) summary: OnceCell<summary::Packages>,
    pub(crate) _phantom: std::marker::PhantomData<K>,
}

macro_rules! shared_comments {
    () => {
        "
Extra functionality is provided in the case that the `Model` had source information
(`WithSource`) or did not (`WithoutSource`). If you need to \"forget\" which case you are in,
`to_any` and `as_any` return a common type that can let values with different source information
to be in tandem, e.g. as different arms in an `if-else`.
Conversely, if you need to \"remember\" which case you are in, you can use `kind` to to case on
the presence source information. This can let you access the extra functionality provided by
the `source_model` or `compiled_model`.
"
    };
}

/// Represents the model data for a package.
#[doc = shared_comments!()]
pub struct Package<'a, K: SourceKind> {
    pub(crate) addr: AccountAddress,
    // TODO name. We likely want the package name from the root package's named address map
    pub(crate) model: &'a Model<K>,
    pub(crate) compiled: &'a normalized::Package,
    pub(crate) data: &'a PackageData<K>,
}

/// Represents the model data for a module.
#[doc = shared_comments!()]
pub struct Module<'a, K: SourceKind> {
    pub(crate) id: ModuleId,
    pub(crate) package: Package<'a, K>,
    pub(crate) compiled: &'a normalized::Module,
    pub(crate) data: &'a ModuleData<K>,
}

/// Represents the model data for a module member.
#[doc = shared_comments!()]
pub enum Member<'a, K: SourceKind> {
    Struct(Struct<'a, K>),
    Enum(Enum<'a, K>),
    Function(Function<'a, K>),
    NamedConstant(source_model::NamedConstant<'a>),
}

/// Represents the model data for a module type declaration (struct or enum).
#[doc = shared_comments!()]
pub enum Datatype<'a, K: SourceKind> {
    Struct(Struct<'a, K>),
    Enum(Enum<'a, K>),
}

/// Represents the model data for a struct declaration.
#[doc = shared_comments!()]
pub struct Struct<'a, K: SourceKind> {
    pub(crate) name: Symbol,
    pub(crate) module: Module<'a, K>,
    pub(crate) compiled: &'a normalized::Struct,
    #[allow(unused)]
    pub(crate) data: &'a StructData,
}

/// Represents the model data for an enum declaration.
#[doc = shared_comments!()]
pub struct Enum<'a, K: SourceKind> {
    pub(crate) name: Symbol,
    pub(crate) module: Module<'a, K>,
    pub(crate) compiled: &'a normalized::Enum,
    #[allow(unused)]
    pub(crate) data: &'a EnumData,
}

/// Represents the model data for an enum's variant declaration.
#[doc = shared_comments!()]
pub struct Variant<'a, K: SourceKind> {
    pub(crate) name: Symbol,
    pub(crate) enum_: Enum<'a, K>,
    pub(crate) compiled: &'a normalized::Variant,
}

/// Represents the model data for a function declaration.
#[doc = shared_comments!()]
pub struct Function<'a, K: SourceKind> {
    pub(crate) name: Symbol,
    pub(crate) module: Module<'a, K>,
    // might be none for macros
    pub(crate) compiled: Option<&'a normalized::Function>,
    #[allow(unused)]
    pub(crate) data: &'a FunctionData,
}

/// Represents the model data for a module's constant present in the `CompiledModule`. Not all
/// constants at the source level are present in the `CompiledModule` depending on optimizations.
/// For source level constants, see `source_model::NamedConstant` and `source_model::Constant`.
#[doc = shared_comments!()]
pub struct CompiledConstant<'a, K: SourceKind> {
    pub(crate) module: Module<'a, K>,
    pub(crate) compiled: &'a normalized::Constant,
    pub(crate) data: &'a ConstantData,
}

//**************************************************************************************************
// API
//**************************************************************************************************

impl<K: SourceKind> Model<K> {
    pub fn root_package_name(&self) -> Option<Symbol> {
        self.root_package_name
    }

    pub fn maybe_package<'a>(&'a self, addr: &AccountAddress) -> Option<Package<'a, K>> {
        let data = self.packages.get(addr)?;
        Some(Package {
            addr: *addr,
            model: self,
            compiled: &self.compiled.packages[addr],
            data,
        })
    }
    pub fn package<'a>(&'a self, addr: &AccountAddress) -> Package<'a, K> {
        self.maybe_package(addr).unwrap()
    }

    /// The name of the package corresponds to the name for the address in the root package's
    /// named address map. This is not the name of the package in the Move.toml file.
    pub fn package_by_name<'a>(&'a self, name: &Symbol) -> Option<Package<'a, K>> {
        let addr = self.root_named_address_map.get(name)?;
        self.maybe_package(addr)
    }

    pub fn maybe_module(&self, module: impl TModuleId) -> Option<Module<'_, K>> {
        let ModuleId { address, name } = module.module_id();
        let package = self.maybe_package(&address)?;
        package.maybe_module(name)
    }

    pub fn module(&self, module: impl TModuleId) -> Module<'_, K> {
        self.maybe_module(module).unwrap()
    }

    pub fn packages(&self) -> impl Iterator<Item = Package<'_, K>> {
        self.packages.keys().map(|a| self.package(a))
    }

    pub fn modules(&self) -> impl Iterator<Item = Module<'_, K>> {
        self.packages
            .iter()
            .flat_map(move |(a, p)| p.modules.keys().map(move |m| self.module((a, m))))
    }

    pub fn compiled_packages(&self) -> &normalized::Packages {
        &self.compiled
    }

    pub fn summary(&self) -> &summary::Packages {
        match self.kind() {
            Kind::WithSource(model) => model.summary_with_source(),
            Kind::WithoutSource(model) => model.summary_without_source(),
        }
    }

    pub fn kind(&self) -> Kind<&Model<WithSource>, &Model<WithoutSource>> {
        if self.has_source() {
            Kind::WithSource(unsafe { std::mem::transmute::<&Self, &Model<WithSource>>(self) })
        } else {
            Kind::WithoutSource(unsafe {
                std::mem::transmute::<&Self, &Model<WithoutSource>>(self)
            })
        }
    }

    pub fn as_any(&self) -> &Model<AnyKind> {
        unsafe { std::mem::transmute::<&Self, &Model<AnyKind>>(self) }
    }

    pub(crate) fn check_invariants(&self) {
        #[cfg(debug_assertions)]
        {
            for (p, package) in &self.packages {
                for (m, module) in &package.modules {
                    let compiled = &self.compiled.packages[p].modules[m];
                    for (idx, s) in module.structs.keys().enumerate() {
                        let map_idx = module.structs.get_index_of(s).unwrap();
                        let compiled_map_idx = compiled.structs.get_index_of(s).unwrap();
                        debug_assert_eq!(idx, map_idx);
                        debug_assert_eq!(idx, compiled_map_idx);
                    }
                    for (idx, f) in module.functions.keys().enumerate() {
                        let map_idx = module.functions.get_index_of(f).unwrap();
                        debug_assert_eq!(idx, map_idx);
                        if let Some(compiled_map_idx) = compiled.functions.get_index_of(f) {
                            debug_assert!(idx >= compiled_map_idx);
                        }
                        if let Kind::WithSource(model) = self.kind() {
                            let module = unsafe {
                                std::mem::transmute::<&ModuleData<K>, &ModuleData<WithSource>>(
                                    module,
                                )
                            };
                            let declared_idx = model
                                .info
                                .module(&module.ident)
                                .functions
                                .get_(f)
                                .unwrap()
                                .index;
                            debug_assert_eq!(idx, declared_idx);
                        }
                    }
                    for (idx, (e, enum_)) in module.enums.iter().enumerate() {
                        let map_idx = module.enums.get_index_of(e).unwrap();
                        let compiled_map_idx = compiled.enums.get_index_of(e).unwrap();
                        debug_assert_eq!(idx, map_idx);
                        debug_assert_eq!(idx, compiled_map_idx);
                        for (vidx, v) in enum_.variants.keys().enumerate() {
                            let map_idx = enum_.variants.get_index_of(v).unwrap();
                            let compiled_map_idx =
                                compiled.enums[e].variants.get_index_of(v).unwrap();
                            debug_assert_eq!(vidx, map_idx);
                            debug_assert_eq!(vidx, compiled_map_idx);
                        }
                    }
                }
            }
        }
    }

    fn has_source(&self) -> bool {
        self.has_source
    }
}

impl<'a, K: SourceKind> Package<'a, K> {
    pub fn address(&self) -> AccountAddress {
        self.addr
    }

    /// The name of the package corresponds to the name for the address in the root package's
    /// named address map. This is not the name of the package in the Move.toml file.
    pub fn name(&self) -> Option<Symbol> {
        self.data.name
    }

    pub fn model(&self) -> &'a Model<K> {
        self.model
    }

    pub fn maybe_module(&self, name: impl Into<Symbol>) -> Option<Module<'a, K>> {
        let name = name.into();
        let data = self.data.modules.get(&name)?;
        Some(Module {
            id: (self.addr, name).module_id(),
            package: *self,
            compiled: &self.compiled.modules[&name],
            data,
        })
    }
    pub fn module(&self, name: impl Into<Symbol>) -> Module<'a, K> {
        self.maybe_module(name).unwrap()
    }

    pub fn modules(&self) -> impl Iterator<Item = Module<'a, K>> + '_ {
        self.data.modules.keys().map(move |name| self.module(*name))
    }

    pub fn compiled(&self) -> &'a normalized::Package {
        self.compiled
    }

    pub fn summary(&self) -> &'a summary::Package {
        &self.model().summary().packages[&self.addr]
    }

    pub fn kind(self) -> Kind<Package<'a, WithSource>, Package<'a, WithoutSource>> {
        if self.model().has_source() {
            Kind::WithSource(unsafe { std::mem::transmute::<Self, Package<'a, WithSource>>(self) })
        } else {
            Kind::WithoutSource(unsafe {
                std::mem::transmute::<Self, Package<'a, WithoutSource>>(self)
            })
        }
    }
}

impl<'a, K: SourceKind> Module<'a, K> {
    pub fn model(&self) -> &'a Model<K> {
        self.package.model()
    }

    pub fn package(&self) -> Package<'a, K> {
        self.package
    }

    pub fn maybe_struct(&self, name: impl Into<Symbol>) -> Option<Struct<'a, K>> {
        let name = name.into();
        let data = &self.data.structs.get(&name)?;
        Some(Struct {
            name,
            module: *self,
            compiled: &self.compiled.structs[&name],
            data,
        })
    }
    pub fn struct_(&self, name: impl Into<Symbol>) -> Struct<'a, K> {
        self.maybe_struct(name).unwrap()
    }

    pub fn maybe_enum(&self, name: impl Into<Symbol>) -> Option<Enum<'a, K>> {
        let name = name.into();
        let data = &self.data.enums.get(&name)?;
        Some(Enum {
            name,
            module: *self,
            compiled: &self.compiled.enums[&name],
            data,
        })
    }
    pub fn enum_(&self, name: impl Into<Symbol>) -> Enum<'a, K> {
        self.maybe_enum(name).unwrap()
    }

    pub fn maybe_function(&self, name: impl Into<Symbol>) -> Option<Function<'a, K>> {
        let name = name.into();
        let data = &self.data.functions.get(&name)?;
        Some(Function {
            name,
            module: *self,
            compiled: self.compiled.functions.get(&name).map(|f| &**f),
            data,
        })
    }
    pub fn function(&self, name: impl Into<Symbol>) -> Function<'a, K> {
        self.maybe_function(name).unwrap()
    }

    pub fn maybe_datatype(&self, name: impl Into<Symbol>) -> Option<Datatype<'a, K>> {
        let name = name.into();
        self.maybe_struct(name)
            .map(Datatype::Struct)
            .or_else(|| self.maybe_enum(name).map(Datatype::Enum))
    }

    pub fn datatype(&self, name: impl Into<Symbol>) -> Datatype<'a, K> {
        self.maybe_datatype(name).unwrap()
    }

    pub fn structs(&self) -> impl Iterator<Item = Struct<'a, K>> + '_ {
        self.data.structs.keys().map(|name| self.struct_(*name))
    }

    pub fn enums(&self) -> impl Iterator<Item = Enum<'a, K>> + '_ {
        self.data.enums.keys().map(|name| self.enum_(*name))
    }

    pub fn functions(&self) -> impl Iterator<Item = Function<'a, K>> + '_ {
        self.data.functions.keys().map(|name| self.function(*name))
    }

    pub fn datatypes(&self) -> impl Iterator<Item = Datatype<'a, K>> + '_ {
        self.structs()
            .map(Datatype::Struct)
            .chain(self.enums().map(Datatype::Enum))
    }

    pub fn compiled_constants(&self) -> impl Iterator<Item = CompiledConstant<'a, K>> + '_ {
        self.compiled
            .constants
            .iter()
            .enumerate()
            .map(|(idx, compiled)| CompiledConstant {
                module: *self,
                compiled,
                data: &self.data.constants[idx],
            })
    }

    pub fn compiled(&self) -> &'a normalized::Module {
        &self.model().compiled.packages[&self.package.addr].modules[&self.name()]
    }

    pub fn name(&self) -> Symbol {
        *self.compiled.name()
    }

    pub fn id(&self) -> ModuleId {
        self.id
    }

    pub fn deps(&self) -> &'a BTreeMap<ModuleId, /* is immediate */ bool> {
        &self.data.deps
    }

    pub fn used_by(&self) -> &'a BTreeMap<ModuleId, /* is immediate */ bool> {
        &self.data.used_by
    }

    pub fn summary(&self) -> &summary::Module {
        &self.package.summary().modules[&self.name()]
    }

    pub fn kind(self) -> Kind<Module<'a, WithSource>, Module<'a, WithoutSource>> {
        if self.model().has_source() {
            Kind::WithSource(unsafe { std::mem::transmute::<Self, Module<'a, WithSource>>(self) })
        } else {
            Kind::WithoutSource(unsafe {
                std::mem::transmute::<Self, Module<'a, WithoutSource>>(self)
            })
        }
    }
}

impl<'a, K: SourceKind> Struct<'a, K> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn model(&self) -> &'a Model<K> {
        self.module.model()
    }

    pub fn package(&self) -> Package<'a, K> {
        self.module.package()
    }

    pub fn module(&self) -> Module<'a, K> {
        self.module
    }

    pub fn compiled(&self) -> &'a normalized::Struct {
        self.compiled
    }

    pub fn summary(&self) -> &summary::Struct {
        &self.module.summary().structs[&self.name]
    }

    pub fn kind(self) -> Kind<Struct<'a, WithSource>, Struct<'a, WithoutSource>> {
        if self.model().has_source() {
            Kind::WithSource(unsafe { std::mem::transmute::<Self, Struct<'a, WithSource>>(self) })
        } else {
            Kind::WithoutSource(unsafe {
                std::mem::transmute::<Self, Struct<'a, WithoutSource>>(self)
            })
        }
    }
}

impl<'a, K: SourceKind> Enum<'a, K> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package(&self) -> Package<'a, K> {
        self.module.package()
    }

    pub fn model(&self) -> &'a Model<K> {
        self.module.model()
    }

    pub fn module(&self) -> Module<'a, K> {
        self.module
    }

    pub fn compiled(&self) -> &'a normalized::Enum {
        self.compiled
    }

    pub fn variants(&self) -> impl Iterator<Item = Variant<'a, K>> + '_ {
        self.compiled
            .variants
            .keys()
            .map(move |name| self.variant(*name))
    }

    pub fn variant(&self, name: Symbol) -> Variant<'a, K> {
        Variant {
            name,
            enum_: *self,
            compiled: &self.compiled.variants[&name],
        }
    }

    pub fn summary(&self) -> &summary::Enum {
        &self.module.summary().enums[&self.name]
    }
}

impl<'a, K: SourceKind> Variant<'a, K> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package(&self) -> Package<'a, K> {
        self.enum_.package()
    }

    pub fn model(&self) -> &'a Model<K> {
        self.enum_.model()
    }

    pub fn module(&self) -> Module<'a, K> {
        self.enum_.module()
    }

    pub fn enum_(&self) -> Enum<'a, K> {
        self.enum_
    }

    pub fn compiled(&self) -> &'a normalized::Variant {
        self.compiled
    }

    pub fn summary(&self) -> &summary::Variant {
        &self.enum_.summary().variants[&self.name]
    }

    pub fn kind(self) -> Kind<Variant<'a, WithSource>, Variant<'a, WithoutSource>> {
        if self.model().has_source() {
            Kind::WithSource(unsafe { std::mem::transmute::<Self, Variant<'a, WithSource>>(self) })
        } else {
            Kind::WithoutSource(unsafe {
                std::mem::transmute::<Self, Variant<'a, WithoutSource>>(self)
            })
        }
    }
}

impl<'a, K: SourceKind> Function<'a, K> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package(&self) -> Package<'a, K> {
        self.module.package()
    }

    pub fn model(&self) -> &'a Model<K> {
        self.module.model()
    }

    pub fn module(&self) -> Module<'a, K> {
        self.module
    }

    /// Returns the compiled function if it exists. This will be `None` for `macro`s.
    pub fn maybe_compiled(&self) -> Option<&'a normalized::Function> {
        self.compiled
    }

    /// Returns an the functions called by this function. This will be empty for `macro`s.
    pub fn calls(&self) -> &'a BTreeSet<QualifiedMemberId> {
        &self.data.calls
    }

    /// Returns the functions that call this function. This will be empty for `macro`s.
    pub fn called_by(&self) -> &'a BTreeSet<QualifiedMemberId> {
        &self.data.called_by
    }

    pub fn summary(&self) -> &summary::Function {
        &self.module.summary().functions[&self.name]
    }

    pub fn kind(self) -> Kind<Function<'a, WithSource>, Function<'a, WithoutSource>> {
        if self.model().has_source() {
            Kind::WithSource(unsafe { std::mem::transmute::<Self, Function<'a, WithSource>>(self) })
        } else {
            Kind::WithoutSource(unsafe {
                std::mem::transmute::<Self, Function<'a, WithoutSource>>(self)
            })
        }
    }
}

impl<'a, K: SourceKind> CompiledConstant<'a, K> {
    pub fn module(&self) -> Module<'a, K> {
        self.module
    }

    pub fn compiled(&self) -> &'a normalized::Constant {
        self.compiled
    }

    pub fn value(&self) -> &'a runtime_value::MoveValue {
        self.data.value(self.compiled)
    }
}

//**************************************************************************************************
// Traits
//**************************************************************************************************

impl TModuleId for ModuleId {
    fn module_id(&self) -> ModuleId {
        *self
    }
}

impl TModuleId for &ModuleId {
    fn module_id(&self) -> ModuleId {
        **self
    }
}

impl TModuleId for (AccountAddress, Symbol) {
    fn module_id(&self) -> ModuleId {
        ModuleId {
            address: self.0,
            name: self.1,
        }
    }
}

impl TModuleId for (&AccountAddress, &Symbol) {
    fn module_id(&self) -> ModuleId {
        ModuleId {
            address: *self.0,
            name: *self.1,
        }
    }
}

impl TModuleId for (NumericalAddress, Symbol) {
    fn module_id(&self) -> ModuleId {
        ModuleId {
            address: self.0.into_inner(),
            name: self.1,
        }
    }
}

impl TModuleId for (&NumericalAddress, &Symbol) {
    fn module_id(&self) -> ModuleId {
        ModuleId {
            address: self.0.into_inner(),
            name: *self.1,
        }
    }
}
impl TModuleId for ModuleIdent_ {
    fn module_id(&self) -> ModuleId {
        let address = self.address.into_addr_bytes().into_inner();
        let name = self.module.0.value;
        ModuleId { address, name }
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
pub(crate) struct PackageData<K: SourceKind> {
    // Based on the root packages named address map
    pub(crate) name: Option<Symbol>,
    pub(crate) modules: BTreeMap<Symbol, ModuleData<K>>,
}

pub(crate) struct ModuleData<K: SourceKind> {
    pub(crate) ident: K::FromSource<E::ModuleIdent>,
    pub(crate) structs: IndexMap<Symbol, StructData>,
    pub(crate) enums: IndexMap<Symbol, EnumData>,
    pub(crate) functions: IndexMap<Symbol, FunctionData>,
    pub(crate) constants: Vec<ConstantData>,
    pub(crate) named_constants: K::FromSource<IndexMap<Symbol, NamedConstantData>>,
    // mapping from file_format::ConstantPoolIndex to source constant name, if any
    pub(crate) constant_names: K::FromSource<Vec<Option<Symbol>>>,
    pub(crate) deps: BTreeMap<ModuleId, /* is immediate */ bool>,
    pub(crate) used_by: BTreeMap<ModuleId, /* is immediate */ bool>,
    pub(crate) _phantom: std::marker::PhantomData<K>,
}

pub(crate) struct StructData {}

pub(crate) struct EnumData {
    #[allow(unused)]
    pub(crate) variants: IndexMap<Symbol, VariantData>,
}

pub(crate) struct VariantData {}

pub(crate) struct FunctionData {
    pub(crate) calls: BTreeSet<QualifiedMemberId>,
    // reverse mapping of function_immediate_deps
    pub(crate) called_by: BTreeSet<QualifiedMemberId>,
}

pub(crate) struct ConstantData {
    pub(crate) value: OnceCell<runtime_value::MoveValue>,
}

pub(crate) struct NamedConstantData {
    pub(crate) compiled_index: Option<file_format::ConstantPoolIndex>,
}

//**************************************************************************************************
// Construction
//**************************************************************************************************

impl<K: SourceKind> Model<K> {
    pub(crate) fn compute_dependencies(&mut self) {
        fn visit(
            packages: &BTreeMap<AccountAddress, normalized::Package>,
            acc: &mut BTreeMap<ModuleId, BTreeMap<ModuleId, bool>>,
            id: ModuleId,
            module: &normalized::Module,
        ) {
            if acc.contains_key(&id) {
                return;
            }

            for immediate_dep in &module.immediate_dependencies {
                let unit = &packages[&immediate_dep.address].modules[&immediate_dep.name];
                visit(packages, acc, *immediate_dep, unit);
            }
            let mut deps = BTreeMap::new();
            for immediate_dep in &module.immediate_dependencies {
                deps.insert(*immediate_dep, true);
                for transitive_dep in acc.get(immediate_dep).unwrap().keys() {
                    if !deps.contains_key(transitive_dep) {
                        deps.insert(*transitive_dep, false);
                    }
                }
            }
            acc.insert(id, deps);
        }

        assert!(self.packages.values().all(|p| {
            p.modules
                .values()
                .all(|m| m.deps.is_empty() && m.used_by.is_empty())
        }));
        let mut module_deps = BTreeMap::new();
        for (a, package) in &self.compiled.packages {
            for (m, module) in &package.modules {
                let id = (a, m).module_id();
                visit(&self.compiled.packages, &mut module_deps, id, module);
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
                let id = (a, m).module_id();
                data.deps = module_deps.remove(&id).unwrap();
                data.used_by = module_used_by.remove(&id).unwrap();
            }
        }
    }

    pub(crate) fn compute_function_dependencies(&mut self) {
        assert!(self.packages.values().all(|p| p.modules.values().all(|m| {
            m.functions
                .values()
                .all(|f| f.calls.is_empty() && f.called_by.is_empty())
        })));
        let mut function_immediate_deps: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
        let modules = self
            .compiled
            .packages
            .iter()
            .flat_map(|(a, p)| p.modules.iter().map(move |(m, u)| ((a, m).module_id(), u)));
        for (id, module) in modules {
            for fdef in module.functions.values() {
                let fname = fdef.name;
                let qualified_id = (id, fname);
                let callees = fdef
                    .code()
                    .iter()
                    .filter_map(|instr| match instr {
                        normalized::Bytecode::Call(callee) => {
                            Some((callee.module, callee.function))
                        }
                        _ => None,
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
                let id = (a, m).module_id();
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

impl PackageData<WithSource> {
    pub(crate) fn from_source(
        name: Option<Symbol>,
        addr: AccountAddress,
        ident_map: &BTreeMap<ModuleId, E::ModuleIdent>,
        info: &TypingProgramInfo,
        named_units: &BTreeMap<Symbol, NamedCompiledModule>,
        units: &normalized::Package,
    ) -> Self {
        let modules = units
            .modules
            .iter()
            .map(|(name, unit)| {
                let id = (addr, *name).module_id();
                let ident = ident_map.get(&id).unwrap();
                let info = info.module(ident);
                let data =
                    ModuleData::from_source(id, *ident, info, unit, named_units[name].source_map());
                (*name, data)
            })
            .collect();
        Self { name, modules }
    }
}

impl PackageData<WithoutSource> {
    pub(crate) fn from_compiled(
        named_address_reverse_map: &BTreeMap<AccountAddress, Symbol>,
        compiled: &normalized::Package,
    ) -> Self {
        let modules = compiled
            .modules
            .iter()
            .map(|(name, unit)| (*name, ModuleData::from_compiled(unit)))
            .collect();
        Self {
            name: named_address_reverse_map.get(&compiled.package).copied(),
            modules,
        }
    }
}

impl ModuleData<WithSource> {
    fn from_source(
        _id: ModuleId,
        ident: E::ModuleIdent,
        info: &ModuleInfo,
        unit: &normalized::Module,
        source_map: &SourceMap,
    ) -> Self {
        let structs = make_map(info.structs.iter().map(|(_loc, name, _sinfo)| {
            let idx = unit.structs.get_index_of(name).unwrap();
            let struct_ = StructData::new();
            (idx, *name, struct_)
        }));
        let enums = make_map(info.enums.iter().map(|(_loc, name, _einfo)| {
            let idx = unit.enums.get_index_of(name).unwrap();
            let enum_ = EnumData::new(unit, &unit.enums[name]);
            (idx, *name, enum_)
        }));
        let functions = make_map(info.functions.iter().map(|(_loc, name, finfo)| {
            let name = *name;
            let function = FunctionData::new();
            (finfo.index, name, function)
        }));
        let constants = unit.constants.iter().map(|_| ConstantData::new()).collect();
        let named_constants = make_map(info.constants.iter().map(|(_loc, name, cinfo)| {
            let name = *name;
            let constant = NamedConstantData::from_source(source_map, name);
            (cinfo.index, name, constant)
        }));
        let constant_names = {
            let idx_to_name_map = source_map
                .constant_map
                .iter()
                .map(|(name, idx)| (*idx, name.0))
                .collect::<BTreeMap<_, _>>();
            let n = unit.constants.len();
            (0..n)
                .map(|i| idx_to_name_map.get(&(i as u16)).copied())
                .collect()
        };
        Self {
            ident,
            structs,
            enums,
            functions,
            constants,
            named_constants,
            constant_names,
            // computed later
            deps: BTreeMap::new(),
            used_by: BTreeMap::new(),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl ModuleData<WithoutSource> {
    fn from_compiled(unit: &normalized::Module) -> Self {
        let structs = unit
            .structs
            .keys()
            .copied()
            .map(|name| (name, StructData::new()))
            .collect();
        let enums = unit
            .enums
            .keys()
            .copied()
            .map(|name| {
                let enum_def = &unit.enums[&name];
                (name, EnumData::new(unit, enum_def))
            })
            .collect();
        let constants = unit.constants.iter().map(|_| ConstantData::new()).collect();
        let functions = unit
            .functions
            .keys()
            .copied()
            .map(|name| (name, FunctionData::new()))
            .collect();
        Self {
            ident: Uninit::new(),
            structs,
            enums,
            functions,
            constants,
            named_constants: Uninit::new(),
            constant_names: Uninit::new(),
            // computed later
            deps: BTreeMap::new(),
            used_by: BTreeMap::new(),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl StructData {
    fn new() -> Self {
        Self {}
    }
}

impl EnumData {
    fn new(_module: &normalized::Module, def: &normalized::Enum) -> Self {
        let mut variants = IndexMap::new();
        for (name, _variant) in &def.variants {
            let name = *name;
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
        Self {
            // computed later
            calls: BTreeSet::new(),
            called_by: BTreeSet::new(),
        }
    }
}

impl ConstantData {
    fn new() -> Self {
        Self {
            value: OnceCell::new(),
        }
    }
}

impl NamedConstantData {
    fn from_source(source_map: &SourceMap, name: Symbol) -> Self {
        let compiled_index = source_map
            .constant_map
            .get(&IR::ConstantName(name))
            .copied()
            .map(file_format::ConstantPoolIndex);
        Self { compiled_index }
    }
}

fn make_map<I: Ord + Copy, T>(
    items: impl IntoIterator<Item = (I, Symbol, T)>,
) -> IndexMap<Symbol, T> {
    let mut items = items.into_iter().collect::<Vec<_>>();
    items.sort_by_key(|(idx, _name, _data)| *idx);
    items
        .into_iter()
        .map(|(_idx, name, data)| (name, data))
        .collect::<IndexMap<_, _>>()
}

//**************************************************************************************************
// Data operations
//**************************************************************************************************

impl ConstantData {
    fn value(&self, compiled: &normalized::Constant) -> &runtime_value::MoveValue {
        self.value.get_or_init(|| {
            let constant_layout = annotated_constant_layout(&compiled.type_);
            runtime_value::MoveValue::simple_deserialize(&compiled.data, &constant_layout).unwrap()
        })
    }
}

fn annotated_constant_layout(ty: &normalized::Type) -> runtime_value::MoveTypeLayout {
    use normalized::Type as T;
    use runtime_value::MoveTypeLayout as L;
    match ty {
        T::Bool => L::Bool,
        T::U8 => L::U8,
        T::U16 => L::U16,
        T::U32 => L::U16,
        T::U64 => L::U64,
        T::U128 => L::U128,
        T::U256 => L::U16,
        T::Address => L::Address,
        T::Vector(inner) => L::Vector(Box::new(annotated_constant_layout(inner))),

        T::Datatype(_) | T::Reference(_, _) | T::TypeParameter(_) | T::Signer => {
            unreachable!("{ty:?} is not supported in constants")
        }
    }
}

//**************************************************************************************************
// Derive
//**************************************************************************************************

// We derive Clone and Copy manually to avoid needlessly requiring `Clone` and `Copy` on
// `K: SourceKind`. This isn't super important now, but can be very annoying if we
// ever use `dyn SourceKind` in the future.
macro_rules! derive_all {
    ($item:ident) => {
        impl<K: SourceKind> Clone for $item<'_, K> {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<K: SourceKind> Copy for $item<'_, K> {}

        impl<'a, K: SourceKind> $item<'a, K> {
            pub fn as_any(&self) -> &$item<'a, AnyKind> {
                unsafe { std::mem::transmute::<&$item<'a, K>, &$item<'a, AnyKind>>(self) }
            }

            pub fn to_any(self) -> $item<'a, AnyKind> {
                unsafe { std::mem::transmute::<$item<'a, K>, $item<'a, AnyKind>>(self) }
            }
        }
    };
}

derive_all!(Package);
derive_all!(Module);
derive_all!(Member);
derive_all!(Datatype);
derive_all!(Struct);
derive_all!(Enum);
derive_all!(Variant);
derive_all!(Function);
derive_all!(CompiledConstant);
