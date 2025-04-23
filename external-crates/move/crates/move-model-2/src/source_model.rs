// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    TModuleId,
    model::{self, NamedConstantData, PackageData},
    normalized,
    source_kind::WithSource,
    summary,
};
use move_compiler::{
    compiled_unit::CompiledUnit,
    expansion::ast as E,
    naming::ast as N,
    shared::{
        files::MappedFiles,
        program_info::{ConstantInfo, FunctionInfo, ModuleInfo, TypingProgramInfo},
    },
};
use move_core_types::{account_address::AccountAddress, runtime_value};
use move_symbol_pool::Symbol;
use std::{cell::OnceCell, collections::BTreeMap, path::PathBuf, sync::Arc};

pub type Model = model::Model<WithSource>;
pub type Package<'a> = model::Package<'a, WithSource>;
pub type Module<'a> = model::Module<'a, WithSource>;
pub type Member<'a> = model::Member<'a, WithSource>;
pub type Datatype<'a> = model::Datatype<'a, WithSource>;
pub type Struct<'a> = model::Struct<'a, WithSource>;
pub type Enum<'a> = model::Enum<'a, WithSource>;
pub type Variant<'a> = model::Variant<'a, WithSource>;
pub type Function<'a> = model::Function<'a, WithSource>;

pub enum Constant<'a> {
    Compiled(CompiledConstant<'a>),
    Named(NamedConstant<'a>),
}

pub type CompiledConstant<'a> = model::CompiledConstant<'a, WithSource>;

pub struct NamedConstant<'a> {
    pub(crate) name: Symbol,
    pub(crate) module: Module<'a>,
    // There is no guarantee a source constant will have a compiled representation
    pub(crate) compiled: Option<&'a normalized::Constant>,
    #[allow(unused)]
    pub(crate) data: &'a NamedConstantData,
}

//**************************************************************************************************
// API
//**************************************************************************************************

impl Model {
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
            has_source: true,
            files,
            root_package_name,
            root_named_address_map,
            info,
            compiled,
            packages,
            summary: OnceCell::new(),
            _phantom: std::marker::PhantomData,
        };
        model.compute_dependencies();
        model.compute_function_dependencies();
        model.check_invariants();
        Ok(model)
    }

    pub fn files(&self) -> &MappedFiles {
        &self.files
    }

    pub fn summary_with_source(&self) -> &summary::Packages {
        self.summary.get_or_init(|| {
            let mut info = summary::Packages::from(&self.compiled);
            info.annotate(self);
            info
        })
    }
}

impl<'a> Module<'a> {
    pub fn ident(&self) -> &'a E::ModuleIdent {
        &self.data.ident
    }

    pub fn info(&self) -> &'a ModuleInfo {
        self.model().info.modules.get(self.ident()).unwrap()
    }

    pub fn source_path(&self) -> Symbol {
        self.model()
            .files
            .filename(&self.info().defined_loc.file_hash())
    }

    pub fn maybe_member(&self, name: impl Into<Symbol>) -> Option<Member<'a>> {
        let name = name.into();
        self.maybe_struct(name)
            .map(Member::Struct)
            .or_else(|| self.maybe_enum(name).map(Member::Enum))
            .or_else(|| self.maybe_function(name).map(Member::Function))
            .or_else(|| self.maybe_named_constant(name).map(Member::NamedConstant))
    }

    pub fn member(&self, name: impl Into<Symbol>) -> Member<'a> {
        self.maybe_member(name).unwrap()
    }

    pub fn maybe_named_constant(&self, name: impl Into<Symbol>) -> Option<NamedConstant<'a>> {
        let name = name.into();
        let data = &self.data.named_constants.get(&name)?;
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
        self.data
            .named_constants
            .keys()
            .copied()
            .map(|name| self.named_constant(name))
    }

    pub fn constants(&self) -> impl Iterator<Item = Constant<'a>> + '_ {
        self.compiled
            .constants
            .iter()
            .enumerate()
            .map(|(idx, compiled)| match self.data.constant_names[idx] {
                Some(name) => Constant::Named(self.named_constant(name)),
                None => Constant::Compiled(CompiledConstant {
                    module: *self,
                    compiled,
                    data: &self.data.constants[idx],
                }),
            })
    }
}

impl<'a> Struct<'a> {
    pub fn info(&self) -> &'a N::StructDefinition {
        self.module.info().structs.get_(&self.name).unwrap()
    }
}

impl<'a> Enum<'a> {
    pub fn info(&self) -> &'a N::EnumDefinition {
        self.module.info().enums.get_(&self.name).unwrap()
    }
}

impl<'a> Variant<'a> {
    pub fn info(&self) -> &'a N::VariantDefinition {
        self.enum_.info().variants.get_(&self.name).unwrap()
    }
}

impl<'a> Function<'a> {
    pub fn info(&self) -> &'a FunctionInfo {
        self.module.info().functions.get_(&self.name).unwrap()
    }
}

impl<'a> Constant<'a> {
    pub fn module(&self) -> Module<'a> {
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

impl<'a> NamedConstant<'a> {
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
// Derive
//**************************************************************************************************

impl Clone for Constant<'_> {
    fn clone(&self) -> Self {
        *self
    }
}
impl Copy for Constant<'_> {}

impl Clone for NamedConstant<'_> {
    fn clone(&self) -> Self {
        *self
    }
}
impl Copy for NamedConstant<'_> {}
