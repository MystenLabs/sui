// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    model::{self, PackageData},
    normalized,
    source_kind::{Uninit, WithoutSource},
    summary,
};
use move_binary_format::CompiledModule;
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;
use std::{cell::OnceCell, collections::BTreeMap};

pub type Model = model::Model<WithoutSource>;
pub type Package<'a> = model::Package<'a, WithoutSource>;
pub type Module<'a> = model::Module<'a, WithoutSource>;
pub type Member<'a> = model::Member<'a, WithoutSource>;
pub type Datatype<'a> = model::Datatype<'a, WithoutSource>;
pub type Struct<'a> = model::Struct<'a, WithoutSource>;
pub type Enum<'a> = model::Enum<'a, WithoutSource>;
pub type Variant<'a> = model::Variant<'a, WithoutSource>;
pub type Function<'a> = model::Function<'a, WithoutSource>;
pub type CompiledConstant<'a> = model::CompiledConstant<'a, WithoutSource>;

impl Model {
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
            has_source: false,
            files: Uninit::new(),
            root_package_name: None,
            root_named_address_map,
            root_named_address_reverse_map: named_address_reverse_map.clone(),
            info: Uninit::new(),
            compiled,
            packages,
            summary: OnceCell::new(),
            _phantom: std::marker::PhantomData,
        };
        model.compute_dependencies();
        model.compute_function_dependencies();
        model.check_invariants();
        model
    }

    pub fn summary_without_source(&self) -> &summary::Packages {
        self.summary.get_or_init(|| {
            summary::Packages::from((&self.root_named_address_reverse_map, &self.compiled))
        })
    }
}

impl<'a> Module<'a> {
    pub fn maybe_member(&self, name: impl Into<Symbol>) -> Option<Member<'a>> {
        let name = name.into();
        self.maybe_struct(name)
            .map(Member::Struct)
            .or_else(|| self.maybe_enum(name).map(Member::Enum))
            .or_else(|| self.maybe_function(name).map(Member::Function))
    }

    pub fn member(&self, name: impl Into<Symbol>) -> Member<'a> {
        self.maybe_member(name).unwrap()
    }

    pub fn constants(&self) -> impl Iterator<Item = CompiledConstant<'a>> + '_ {
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

impl<'a> Function<'a> {
    pub fn compiled(&self) -> &'a normalized::Function {
        self.compiled.unwrap()
    }
}
