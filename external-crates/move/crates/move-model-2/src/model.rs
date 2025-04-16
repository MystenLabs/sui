// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

use crate::{
    normalized::{self, ModuleId, QualifiedMemberId, TModuleId},
    serializable_signatures,
};
use indexmap::IndexMap;
use move_binary_format::{file_format, CompiledModule};
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
use move_core_types::{account_address::AccountAddress, runtime_value};
use move_ir_types::ast as IR;
use move_ir_types::location::Spanned;
use move_symbol_pool::Symbol;

//**************************************************************************************************
// Types
//**************************************************************************************************

pub const WITH_SOURCE: SourceKind = 1;
pub const WITHOUT_SOURCE: SourceKind = 0;

pub type SourceKind = usize;

#[derive(Clone, Copy)]
pub enum Kind<TWithSource, TWithout> {
    WithSource(TWithSource),
    WithoutSource(TWithout),
}

pub struct Model<const HAS_SOURCE: SourceKind> {
    files: [MappedFiles; HAS_SOURCE],
    root_named_address_map: BTreeMap<Symbol, AccountAddress>,
    root_package_name: Option<Symbol>,
    info: [Arc<TypingProgramInfo>; HAS_SOURCE],
    compiled: normalized::Packages,
    packages: BTreeMap<AccountAddress, PackageData<HAS_SOURCE>>,
    serializable_signatures: OnceCell<serializable_signatures::Packages>,
}

#[derive(Clone, Copy)]
pub struct Package<'a, const HAS_SOURCE: SourceKind> {
    addr: AccountAddress,
    // TODO name. We likely want the package name from the root package's named address map
    model: &'a Model<HAS_SOURCE>,
    compiled: &'a normalized::Package,
    data: &'a PackageData<HAS_SOURCE>,
}

#[derive(Clone, Copy)]
pub struct Module<'a, const HAS_SOURCE: SourceKind> {
    id: ModuleId,
    package: Package<'a, HAS_SOURCE>,
    compiled: &'a normalized::Module,
    data: &'a ModuleData<HAS_SOURCE>,
}

#[derive(Clone, Copy)]
pub enum Member<'a, const HAS_SOURCE: SourceKind> {
    Struct(Struct<'a, HAS_SOURCE>),
    Enum(Enum<'a, HAS_SOURCE>),
    Function(Function<'a, HAS_SOURCE>),
    NamedConstant(NamedConstant<'a>),
}

#[derive(Clone, Copy)]
pub enum Datatype<'a, const HAS_SOURCE: SourceKind> {
    Struct(Struct<'a, HAS_SOURCE>),
    Enum(Enum<'a, HAS_SOURCE>),
}

#[derive(Clone, Copy)]
pub struct Struct<'a, const HAS_SOURCE: SourceKind> {
    name: Symbol,
    module: Module<'a, HAS_SOURCE>,
    compiled: &'a normalized::Struct,
    #[allow(unused)]
    data: &'a StructData,
}

#[derive(Clone, Copy)]
pub struct Enum<'a, const HAS_SOURCE: SourceKind> {
    name: Symbol,
    module: Module<'a, HAS_SOURCE>,
    compiled: &'a normalized::Enum,
    #[allow(unused)]
    data: &'a EnumData,
}

#[derive(Clone, Copy)]
pub struct Variant<'a, const HAS_SOURCE: SourceKind> {
    name: Symbol,
    enum_: Enum<'a, HAS_SOURCE>,
    compiled: &'a normalized::Variant,
}

#[derive(Clone, Copy)]
pub struct Function<'a, const HAS_SOURCE: SourceKind> {
    name: Symbol,
    module: Module<'a, HAS_SOURCE>,
    // might be none for macros
    compiled: Option<&'a normalized::Function>,
    #[allow(unused)]
    data: &'a FunctionData,
}

#[derive(Clone, Copy)]
pub enum Constant<'a> {
    Compiled(CompiledConstant<'a, WITH_SOURCE>),
    Named(NamedConstant<'a>),
}

#[derive(Clone, Copy)]
pub struct CompiledConstant<'a, const HAS_SOURCE: SourceKind> {
    module: Module<'a, HAS_SOURCE>,
    compiled: &'a normalized::Constant,
    data: &'a ConstantData,
}

#[derive(Clone, Copy)]
pub struct NamedConstant<'a> {
    name: Symbol,
    module: Module<'a, WITH_SOURCE>,
    // There is no guarantee a source constant will have a compiled representation
    compiled: Option<&'a normalized::Constant>,
    #[allow(unused)]
    data: &'a NamedConstantData,
}

//**************************************************************************************************
// API
//**************************************************************************************************

impl Model<WITH_SOURCE> {
    pub fn from_source(
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
            .collect::<BTreeMap<_, BTreeMap<_, _>>>();
        let root_named_address_reverse_map = root_named_address_map
            .iter()
            .map(|(n, a)| (*a, *n))
            .collect::<BTreeMap<_, _>>();
        let ident_map = info
            .modules
            .key_cloned_iter()
            .map(|(ident, _)| (ident.module_id(), ident))
            .collect::<BTreeMap<_, _>>();
        let compiled_modules = compiled_units
            .iter()
            .flat_map(|(_addr, units)| units.values().map(|unit| &unit.module));
        let compiled = normalized::Packages::new(compiled_modules);
        let packages = compiled
            .packages
            .iter()
            .map(|(addr, units)| {
                let name = root_named_address_reverse_map.get(addr).copied();
                let data = PackageData::from_source(
                    name,
                    *addr,
                    &ident_map,
                    &info,
                    &compiled_units[addr],
                    units,
                );
                (*addr, data)
            })
            .collect();
        let mut model = Self {
            files: [files],
            root_package_name,
            root_named_address_map,
            info: [info],
            compiled,
            packages,
            serializable_signatures: OnceCell::new(),
        };
        model.compute_dependencies();
        model.compute_function_dependencies();
        model.check_invariants();
        Ok(model)
    }

    pub fn files(&self) -> &MappedFiles {
        &self.files[0]
    }

    pub fn serializable_signatures(&self) -> &serializable_signatures::Packages {
        self.serializable_signatures.get_or_init(|| {
            let mut info = serializable_signatures::Packages::from(&self.compiled);
            info.annotate(self);
            info
        })
    }
}

impl Model<WITHOUT_SOURCE> {
    pub fn from_compiled(
        named_address_reverse_map: &BTreeMap<AccountAddress, Symbol>,
        modules: Vec<CompiledModule>,
    ) -> Self {
        let compiled = normalized::Packages::new(&modules);
        let packages = compiled
            .packages
            .values()
            .map(|package| {
                let addr = package.package;
                let data = PackageData::from_compiled(named_address_reverse_map, package);
                (addr, data)
            })
            .collect();
        let root_named_address_map = named_address_reverse_map
            .iter()
            .map(|(a, n)| (*n, *a))
            .collect();
        let mut model = Self {
            files: [],
            root_package_name: None,
            root_named_address_map,
            info: [],
            compiled,
            packages,
            serializable_signatures: OnceCell::new(),
        };
        model.compute_dependencies();
        model.compute_function_dependencies();
        model.check_invariants();
        model
    }

    pub fn serializable_signatures(&self) -> &serializable_signatures::Packages {
        self.serializable_signatures
            .get_or_init(|| serializable_signatures::Packages::from(&self.compiled))
    }
}

impl<const HAS_SOURCE: SourceKind> Model<HAS_SOURCE> {
    pub fn root_package_name(&self) -> Option<Symbol> {
        self.root_package_name
    }

    pub fn maybe_package<'a>(&'a self, addr: &AccountAddress) -> Option<Package<'a, HAS_SOURCE>> {
        let data = self.packages.get(addr)?;
        Some(Package {
            addr: *addr,
            model: self,
            compiled: &self.compiled.packages[addr],
            data,
        })
    }
    pub fn package<'a>(&'a self, addr: &AccountAddress) -> Package<'a, HAS_SOURCE> {
        self.maybe_package(addr).unwrap()
    }

    /// The name of the package corresponds to the name for the address in the root package's
    /// named address map. This is not the name of the package in the Move.toml file.
    pub fn package_by_name<'a>(&'a self, name: &Symbol) -> Option<Package<'a, HAS_SOURCE>> {
        let addr = self.root_named_address_map.get(name)?;
        self.maybe_package(addr)
    }

    pub fn maybe_module(&self, module: impl TModuleId) -> Option<Module<'_, HAS_SOURCE>> {
        let ModuleId { address, name } = module.module_id();
        let package = self.maybe_package(&address)?;
        package.maybe_module(name)
    }
    pub fn module(&self, module: impl TModuleId) -> Module<HAS_SOURCE> {
        self.maybe_module(module).unwrap()
    }

    pub fn packages(&self) -> impl Iterator<Item = Package<'_, HAS_SOURCE>> {
        self.packages.keys().map(|a| self.package(a))
    }

    pub fn modules(&self) -> impl Iterator<Item = Module<'_, HAS_SOURCE>> {
        self.packages
            .iter()
            .flat_map(move |(a, p)| p.modules.keys().map(move |m| self.module((a, m))))
    }

    pub fn compiled_packages(&self) -> &normalized::Packages {
        &self.compiled
    }

    pub fn signatures(&self) -> &serializable_signatures::Packages {
        match self.kind() {
            Kind::WithSource(model) => model.serializable_signatures(),
            Kind::WithoutSource(model) => model.serializable_signatures(),
        }
    }

    pub fn kind(&self) -> Kind<&Model<WITH_SOURCE>, &Model<WITHOUT_SOURCE>> {
        match HAS_SOURCE {
            WITH_SOURCE => {
                Kind::WithSource(unsafe { std::mem::transmute::<&Self, &Model<WITH_SOURCE>>(self) })
            }
            WITHOUT_SOURCE => Kind::WithoutSource(unsafe {
                std::mem::transmute::<&Self, &Model<WITHOUT_SOURCE>>(self)
            }),
            _ => unreachable!(),
        }
    }

    fn check_invariants(&self) {
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
                        if HAS_SOURCE == WITH_SOURCE {
                            let declared_idx = self.info[0]
                                .module(&module.ident[0])
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
}

impl<'a, const HAS_SOURCE: SourceKind> Package<'a, HAS_SOURCE> {
    pub fn address(&self) -> AccountAddress {
        self.addr
    }

    /// The name of the package corresponds to the name for the address in the root package's
    /// named address map. This is not the name of the package in the Move.toml file.
    pub fn name(&self) -> Option<Symbol> {
        self.data.name
    }

    pub fn model(&self) -> &'a Model<HAS_SOURCE> {
        self.model
    }

    pub fn maybe_module(&self, name: impl Into<Symbol>) -> Option<Module<'a, HAS_SOURCE>> {
        let name = name.into();
        let data = self.data.modules.get(&name)?;
        Some(Module {
            id: (self.addr, name).module_id(),
            package: *self,
            compiled: &self.compiled.modules[&name],
            data,
        })
    }
    pub fn module(&self, name: impl Into<Symbol>) -> Module<'a, HAS_SOURCE> {
        self.maybe_module(name).unwrap()
    }

    pub fn modules(&self) -> impl Iterator<Item = Module<'a, HAS_SOURCE>> + '_ {
        self.data.modules.keys().map(move |name| self.module(*name))
    }

    pub fn compiled(&self) -> &'a normalized::Package {
        self.compiled
    }

    pub fn signatures(&self) -> &'a serializable_signatures::Package {
        &self.model().signatures().packages[&self.addr]
    }

    pub fn kind(self) -> Kind<Package<'a, WITH_SOURCE>, Package<'a, WITHOUT_SOURCE>> {
        match HAS_SOURCE {
            WITH_SOURCE => Kind::WithSource(unsafe {
                std::mem::transmute::<Self, Package<'a, WITH_SOURCE>>(self)
            }),
            WITHOUT_SOURCE => Kind::WithoutSource(unsafe {
                std::mem::transmute::<Self, Package<'a, WITHOUT_SOURCE>>(self)
            }),
            _ => unreachable!(),
        }
    }
}

impl<'a, const HAS_SOURCE: SourceKind> Module<'a, HAS_SOURCE> {
    pub fn model(&self) -> &'a Model<HAS_SOURCE> {
        self.package.model()
    }

    pub fn package(&self) -> Package<'a, HAS_SOURCE> {
        self.package
    }

    pub fn maybe_struct(&self, name: impl Into<Symbol>) -> Option<Struct<'a, HAS_SOURCE>> {
        let name = name.into();
        let data = &self.data.structs.get(&name)?;
        Some(Struct {
            name,
            module: *self,
            compiled: &self.compiled.structs[&name],
            data,
        })
    }
    pub fn struct_(&self, name: impl Into<Symbol>) -> Struct<'a, HAS_SOURCE> {
        self.maybe_struct(name).unwrap()
    }

    pub fn maybe_enum(&self, name: impl Into<Symbol>) -> Option<Enum<'a, HAS_SOURCE>> {
        let name = name.into();
        let data = &self.data.enums.get(&name)?;
        Some(Enum {
            name,
            module: *self,
            compiled: &self.compiled.enums[&name],
            data,
        })
    }
    pub fn enum_(&self, name: impl Into<Symbol>) -> Enum<'a, HAS_SOURCE> {
        self.maybe_enum(name).unwrap()
    }

    pub fn maybe_function(&self, name: impl Into<Symbol>) -> Option<Function<'a, HAS_SOURCE>> {
        let name = name.into();
        let data = &self.data.functions.get(&name)?;
        Some(Function {
            name,
            module: *self,
            compiled: self.compiled.functions.get(&name).map(|f| &**f),
            data,
        })
    }
    pub fn function(&self, name: impl Into<Symbol>) -> Function<'a, HAS_SOURCE> {
        self.maybe_function(name).unwrap()
    }

    pub fn maybe_datatype(&self, name: impl Into<Symbol>) -> Option<Datatype<'a, HAS_SOURCE>> {
        let name = name.into();
        self.maybe_struct(name)
            .map(Datatype::Struct)
            .or_else(|| self.maybe_enum(name).map(Datatype::Enum))
    }

    pub fn datatype(&self, name: impl Into<Symbol>) -> Datatype<'a, HAS_SOURCE> {
        self.maybe_datatype(name).unwrap()
    }

    pub fn structs(&self) -> impl Iterator<Item = Struct<'a, HAS_SOURCE>> + '_ {
        self.data.structs.keys().map(|name| self.struct_(*name))
    }

    pub fn enums(&self) -> impl Iterator<Item = Enum<'a, HAS_SOURCE>> + '_ {
        self.data.enums.keys().map(|name| self.enum_(*name))
    }

    pub fn functions(&self) -> impl Iterator<Item = Function<'a, HAS_SOURCE>> + '_ {
        self.data.functions.keys().map(|name| self.function(*name))
    }

    pub fn datatypes(&self) -> impl Iterator<Item = Datatype<'a, HAS_SOURCE>> + '_ {
        self.structs()
            .map(Datatype::Struct)
            .chain(self.enums().map(Datatype::Enum))
    }

    pub fn compiled_constants(
        &self,
    ) -> impl Iterator<Item = CompiledConstant<'a, HAS_SOURCE>> + '_ {
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

    pub fn signatures(&self) -> &serializable_signatures::Module {
        &self.package.signatures().modules[&self.name()]
    }

    pub fn kind(self) -> Kind<Module<'a, WITH_SOURCE>, Module<'a, WITHOUT_SOURCE>> {
        match HAS_SOURCE {
            WITH_SOURCE => Kind::WithSource(unsafe {
                std::mem::transmute::<Self, Module<'a, WITH_SOURCE>>(self)
            }),
            WITHOUT_SOURCE => Kind::WithoutSource(unsafe {
                std::mem::transmute::<Self, Module<'a, WITHOUT_SOURCE>>(self)
            }),
            _ => unreachable!(),
        }
    }
}

impl<'a> Module<'a, WITH_SOURCE> {
    pub fn ident(&self) -> &'a E::ModuleIdent {
        &self.data.ident[0]
    }

    pub fn info(&self) -> &'a ModuleInfo {
        self.model().info[0].modules.get(self.ident()).unwrap()
    }

    pub fn source_path(&self) -> Symbol {
        self.model().files[0].filename(&self.info().defined_loc.file_hash())
    }

    pub fn maybe_member(&self, name: impl Into<Symbol>) -> Option<Member<'a, WITH_SOURCE>> {
        let name = name.into();
        self.maybe_struct(name)
            .map(Member::Struct)
            .or_else(|| self.maybe_enum(name).map(Member::Enum))
            .or_else(|| self.maybe_function(name).map(Member::Function))
            .or_else(|| self.maybe_named_constant(name).map(Member::NamedConstant))
    }

    pub fn member(&self, name: impl Into<Symbol>) -> Member<'a, WITH_SOURCE> {
        self.maybe_member(name).unwrap()
    }

    pub fn maybe_named_constant(&self, name: impl Into<Symbol>) -> Option<NamedConstant<'a>> {
        let name = name.into();
        let data = &self.data.named_constants[0].get(&name)?;
        let compiled = data
            .compiled_index
            .map(|idx| &*self.compiled.constants[idx.0 as usize]);
        Some(NamedConstant {
            name,
            module: *self,
            compiled,
            data,
        })
    }

    pub fn named_constant(&self, name: impl Into<Symbol>) -> NamedConstant<'a> {
        self.maybe_named_constant(name).unwrap()
    }

    pub fn named_constants(&self) -> impl Iterator<Item = NamedConstant<'a>> + '_ {
        self.data.named_constants[0]
            .keys()
            .copied()
            .map(|name| self.named_constant(name))
    }

    pub fn constants(&self) -> impl Iterator<Item = Constant<'a>> + '_ {
        self.compiled
            .constants
            .iter()
            .enumerate()
            .map(|(idx, compiled)| match self.data.constant_names[0][idx] {
                Some(name) => Constant::Named(self.named_constant(name)),
                None => Constant::Compiled(CompiledConstant {
                    module: *self,
                    compiled,
                    data: &self.data.constants[idx],
                }),
            })
    }
}

impl<'a> Module<'a, WITHOUT_SOURCE> {
    pub fn maybe_member(&self, name: impl Into<Symbol>) -> Option<Member<'a, WITHOUT_SOURCE>> {
        let name = name.into();
        self.maybe_struct(name)
            .map(Member::Struct)
            .or_else(|| self.maybe_enum(name).map(Member::Enum))
            .or_else(|| self.maybe_function(name).map(Member::Function))
    }

    pub fn member(&self, name: impl Into<Symbol>) -> Member<'a, WITHOUT_SOURCE> {
        self.maybe_member(name).unwrap()
    }

    pub fn constants(&self) -> impl Iterator<Item = CompiledConstant<'a, WITHOUT_SOURCE>> + '_ {
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
}

impl<'a, const HAS_SOURCE: SourceKind> Struct<'a, HAS_SOURCE> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn model(&self) -> &'a Model<HAS_SOURCE> {
        self.module.model()
    }

    pub fn package(&self) -> Package<'a, HAS_SOURCE> {
        self.module.package()
    }

    pub fn module(&self) -> Module<'a, HAS_SOURCE> {
        self.module
    }

    pub fn compiled(&self) -> &'a normalized::Struct {
        self.compiled
    }

    pub fn signature(&self) -> &serializable_signatures::Struct {
        &self.module.signatures().structs[&self.name]
    }

    pub fn kind(self) -> Kind<Struct<'a, WITH_SOURCE>, Struct<'a, WITHOUT_SOURCE>> {
        match HAS_SOURCE {
            WITH_SOURCE => Kind::WithSource(unsafe {
                std::mem::transmute::<Self, Struct<'a, WITH_SOURCE>>(self)
            }),
            WITHOUT_SOURCE => Kind::WithoutSource(unsafe {
                std::mem::transmute::<Self, Struct<'a, WITHOUT_SOURCE>>(self)
            }),
            _ => unreachable!(),
        }
    }
}

impl<'a> Struct<'a, WITH_SOURCE> {
    pub fn info(&self) -> &'a N::StructDefinition {
        self.module.info().structs.get_(&self.name).unwrap()
    }
}

impl<'a, const HAS_SOURCE: SourceKind> Enum<'a, HAS_SOURCE> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package(&self) -> Package<'a, HAS_SOURCE> {
        self.module.package()
    }

    pub fn model(&self) -> &'a Model<HAS_SOURCE> {
        self.module.model()
    }

    pub fn module(&self) -> Module<'a, HAS_SOURCE> {
        self.module
    }

    pub fn compiled(&self) -> &'a normalized::Enum {
        self.compiled
    }

    pub fn variants(&self) -> impl Iterator<Item = Variant<'a, HAS_SOURCE>> + '_ {
        self.compiled
            .variants
            .keys()
            .map(move |name| self.variant(*name))
    }

    pub fn variant(&self, name: Symbol) -> Variant<'a, HAS_SOURCE> {
        Variant {
            name,
            enum_: *self,
            compiled: &self.compiled.variants[&name],
        }
    }

    pub fn signature(&self) -> &serializable_signatures::Enum {
        &self.module.signatures().enums[&self.name]
    }
}

impl<'a> Enum<'a, WITH_SOURCE> {
    pub fn info(&self) -> &'a N::EnumDefinition {
        self.module.info().enums.get_(&self.name).unwrap()
    }
}

impl<'a, const HAS_SOURCE: SourceKind> Variant<'a, HAS_SOURCE> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package(&self) -> Package<'a, HAS_SOURCE> {
        self.enum_.package()
    }

    pub fn model(&self) -> &'a Model<HAS_SOURCE> {
        self.enum_.model()
    }

    pub fn module(&self) -> Module<'a, HAS_SOURCE> {
        self.enum_.module()
    }

    pub fn enum_(&self) -> Enum<'a, HAS_SOURCE> {
        self.enum_
    }

    pub fn compiled(&self) -> &'a normalized::Variant {
        self.compiled
    }

    pub fn signature(&self) -> &serializable_signatures::Variant {
        &self.enum_.signature().variants[&self.name]
    }

    pub fn kind(self) -> Kind<Variant<'a, WITH_SOURCE>, Variant<'a, WITHOUT_SOURCE>> {
        match HAS_SOURCE {
            WITH_SOURCE => Kind::WithSource(unsafe {
                std::mem::transmute::<Self, Variant<'a, WITH_SOURCE>>(self)
            }),
            WITHOUT_SOURCE => Kind::WithoutSource(unsafe {
                std::mem::transmute::<Self, Variant<'a, WITHOUT_SOURCE>>(self)
            }),
            _ => unreachable!(),
        }
    }
}

impl<'a> Variant<'a, WITH_SOURCE> {
    pub fn info(&self) -> &'a N::VariantDefinition {
        self.enum_.info().variants.get_(&self.name).unwrap()
    }
}

impl<'a, const HAS_SOURCE: SourceKind> Function<'a, HAS_SOURCE> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package(&self) -> Package<'a, HAS_SOURCE> {
        self.module.package()
    }

    pub fn model(&self) -> &'a Model<HAS_SOURCE> {
        self.module.model()
    }

    pub fn module(&self) -> Module<'a, HAS_SOURCE> {
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

    pub fn signature(&self) -> &serializable_signatures::Function {
        &self.module.signatures().functions[&self.name]
    }

    pub fn kind(self) -> Kind<Function<'a, WITH_SOURCE>, Function<'a, WITHOUT_SOURCE>> {
        match HAS_SOURCE {
            WITH_SOURCE => Kind::WithSource(unsafe {
                std::mem::transmute::<Self, Function<'a, WITH_SOURCE>>(self)
            }),
            WITHOUT_SOURCE => Kind::WithoutSource(unsafe {
                std::mem::transmute::<Self, Function<'a, WITHOUT_SOURCE>>(self)
            }),
            _ => unreachable!(),
        }
    }
}

impl<'a> Function<'a, WITH_SOURCE> {
    pub fn info(&self) -> &'a FunctionInfo {
        self.module.info().functions.get_(&self.name).unwrap()
    }
}

impl<'a> Function<'a, WITHOUT_SOURCE> {
    pub fn compiled(&self) -> &'a normalized::Function {
        self.compiled.unwrap()
    }
}

impl<'a> Constant<'a> {
    pub fn module(&self) -> Module<'a, WITH_SOURCE> {
        match self {
            Constant::Compiled(c) => c.module,
            Constant::Named(c) => c.module,
        }
    }

    pub fn compiled(&self) -> Option<&'a normalized::Constant> {
        match self {
            Constant::Compiled(c) => Some(c.compiled),
            Constant::Named(c) => c.compiled,
        }
    }

    pub fn value(&self) -> &'a runtime_value::MoveValue {
        match self {
            Constant::Compiled(c) => c.value(),
            Constant::Named(c) => c.value(),
        }
    }
}

impl<'a, const HAS_SOURCE: SourceKind> CompiledConstant<'a, HAS_SOURCE> {
    pub fn module(&self) -> Module<'a, HAS_SOURCE> {
        self.module
    }

    pub fn compiled(&self) -> &'a normalized::Constant {
        self.compiled
    }

    pub fn value(&self) -> &'a runtime_value::MoveValue {
        self.data.value(self.compiled)
    }
}

impl<'a> NamedConstant<'a> {
    pub fn name(&self) -> Symbol {
        self.name
    }

    pub fn package(&self) -> Package<'a, WITH_SOURCE> {
        self.module.package()
    }

    pub fn model(&self) -> &'a Model<WITH_SOURCE> {
        self.module.model()
    }

    pub fn module(&self) -> Module<'a, WITH_SOURCE> {
        self.module
    }

    pub fn info(&self) -> &'a ConstantInfo {
        self.module.info().constants.get_(&self.name).unwrap()
    }

    /// Not all source constants have a compiled representation
    pub fn compiled(&self) -> Option<&'a normalized::Constant> {
        self.compiled
    }

    pub fn value(&self) -> &'a runtime_value::MoveValue {
        // we normally don't write delegates into ProgramInfo, but we are doing so here for parity
        // with CompiledConstant
        self.info().value.get().unwrap()
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
struct PackageData<const HAS_SOURCE: SourceKind> {
    // Based on the root packages named address map
    name: Option<Symbol>,
    modules: BTreeMap<Symbol, ModuleData<HAS_SOURCE>>,
}

struct ModuleData<const HAS_SOURCE: SourceKind> {
    ident: [E::ModuleIdent; HAS_SOURCE],
    structs: IndexMap<Symbol, StructData>,
    enums: IndexMap<Symbol, EnumData>,
    functions: IndexMap<Symbol, FunctionData>,
    constants: Vec<ConstantData>,
    named_constants: [IndexMap<Symbol, NamedConstantData>; HAS_SOURCE],
    // mapping from file_format::ConstantPoolIndex to source constant name, if any
    constant_names: [Vec<Option<Symbol>>; HAS_SOURCE],
    deps: BTreeMap<ModuleId, /* is immediate */ bool>,
    used_by: BTreeMap<ModuleId, /* is immediate */ bool>,
}

struct StructData {}

struct EnumData {
    #[allow(unused)]
    variants: IndexMap<Symbol, VariantData>,
}

struct VariantData {}

struct FunctionData {
    calls: BTreeSet<QualifiedMemberId>,
    // reverse mapping of function_immediate_deps
    called_by: BTreeSet<QualifiedMemberId>,
}

struct ConstantData {
    value: OnceCell<runtime_value::MoveValue>,
}

struct NamedConstantData {
    compiled_index: Option<file_format::ConstantPoolIndex>,
}

//**************************************************************************************************
// Construction
//**************************************************************************************************

impl<const WITH_SOURCE: SourceKind> Model<WITH_SOURCE> {
    fn compute_dependencies(&mut self) {
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

        assert!(self.packages.values().all(|p| p
            .modules
            .values()
            .all(|m| m.deps.is_empty() && m.used_by.is_empty())));
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

    fn compute_function_dependencies(&mut self) {
        assert!(self.packages.values().all(|p| p.modules.values().all(|m| m
            .functions
            .values()
            .all(|f| f.calls.is_empty() && f.called_by.is_empty()))));
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

impl PackageData<WITH_SOURCE> {
    fn from_source(
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

impl PackageData<WITHOUT_SOURCE> {
    fn from_compiled(
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

impl ModuleData<WITH_SOURCE> {
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
            ident: [ident],
            structs,
            enums,
            functions,
            constants,
            named_constants: [named_constants],
            constant_names: [constant_names],
            // computed later
            deps: BTreeMap::new(),
            used_by: BTreeMap::new(),
        }
    }
}

impl ModuleData<WITHOUT_SOURCE> {
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
            ident: [],
            structs,
            enums,
            functions,
            constants,
            named_constants: [],
            constant_names: [],
            // computed later
            deps: BTreeMap::new(),
            used_by: BTreeMap::new(),
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
