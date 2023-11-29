// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod dependency_graph;
pub mod layout;
pub mod module_cache;

use crate::dependency_graph::DependencyGraph;
use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{CompiledModule, DatatypeHandleIndex, SignatureToken},
};
use move_core_types::{
    account_address::AccountAddress, identifier::IdentStr, language_storage::ModuleId,
};

use anyhow::{anyhow, Result};
use std::collections::BTreeMap;

/// Set of Move modules indexed by module Id
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Modules<'a>(BTreeMap<ModuleId, &'a CompiledModule>);

impl<'a> Modules<'a> {
    /// Construct a set of modules from a slice `modules`.
    /// Panics if `modules` contains duplicates
    pub fn new(modules: impl IntoIterator<Item = &'a CompiledModule>) -> Self {
        let mut map = BTreeMap::new();
        for m in modules {
            assert!(
                map.insert(m.self_id(), m).is_none(),
                "Duplicate module found"
            );
        }
        Modules(map)
    }

    /// Return all modules in this set
    pub fn iter_modules(&self) -> Vec<&CompiledModule> {
        self.0.values().copied().collect()
    }

    /// Return all modules in this set
    pub fn iter_modules_owned(&self) -> Vec<CompiledModule> {
        self.iter_modules().into_iter().cloned().collect()
    }

    /// Compute a dependency graph for `self`
    pub fn compute_dependency_graph(&self) -> DependencyGraph {
        DependencyGraph::new(self.0.values().copied())
    }

    /// Return the backing map of `self`
    pub fn get_map(&self) -> &BTreeMap<ModuleId, &CompiledModule> {
        &self.0
    }

    /// Return the bytecode for the module bound to `module_id`
    pub fn get_module(&self, module_id: &ModuleId) -> Result<&CompiledModule> {
        self.0
            .get(module_id)
            .copied()
            .ok_or_else(|| anyhow!("Can't find module {:?}", module_id))
    }

    /// Return the immediate dependencies for `module_id`
    pub fn get_immediate_dependencies(&self, module_id: &ModuleId) -> Result<Vec<&CompiledModule>> {
        self.get_module(module_id)?
            .immediate_dependencies()
            .into_iter()
            .map(|mid| self.get_module(&mid))
            .collect::<Result<Vec<_>>>()
    }

    fn get_transitive_dependencies_(
        &'a self,
        all_deps: &mut Vec<&'a CompiledModule>,
        module: &'a CompiledModule,
    ) -> Result<()> {
        let next_deps = module.immediate_dependencies();
        all_deps.push(module);
        for next in next_deps {
            let next_module = self.get_module(&next)?;
            self.get_transitive_dependencies_(all_deps, next_module)?;
        }
        Ok(())
    }

    /// Return the transitive dependencies for `module_id`
    pub fn get_transitive_dependencies(
        &self,
        module_id: &ModuleId,
    ) -> Result<Vec<&CompiledModule>> {
        let mut all_deps = vec![];
        for dep in self.get_immediate_dependencies(module_id)? {
            self.get_transitive_dependencies_(&mut all_deps, dep)?;
        }
        Ok(all_deps)
    }
}

pub fn resolve_struct<'a>(
    view: &'a BinaryIndexedView,
    sidx: DatatypeHandleIndex,
) -> (&'a AccountAddress, &'a IdentStr, &'a IdentStr) {
    let shandle = view.datatype_handle_at(sidx);
    let mhandle = view.module_handle_at(shandle.module);
    let address = view.address_identifier_at(mhandle.address);
    let module_name = view.identifier_at(mhandle.name);
    let struct_name = view.identifier_at(shandle.name);
    (address, module_name, struct_name)
}

pub fn format_signature_token(view: &BinaryIndexedView, t: &SignatureToken) -> String {
    match t {
        SignatureToken::Bool => "bool".to_string(),
        SignatureToken::U8 => "u8".to_string(),
        SignatureToken::U16 => "u16".to_string(),
        SignatureToken::U32 => "u32".to_string(),
        SignatureToken::U64 => "u64".to_string(),
        SignatureToken::U128 => "u128".to_string(),
        SignatureToken::U256 => "u256".to_string(),
        SignatureToken::Address => "address".to_string(),
        SignatureToken::Signer => "signer".to_string(),
        SignatureToken::Vector(inner) => {
            format!("vector<{}>", format_signature_token(view, inner))
        }
        SignatureToken::Reference(inner) => format!("&{}", format_signature_token(view, inner)),
        SignatureToken::MutableReference(inner) => {
            format!("&mut {}", format_signature_token(view, inner))
        }
        SignatureToken::TypeParameter(i) => format!("T{}", i),

        SignatureToken::Datatype(idx) => format_signature_token_struct(view, *idx, &[]),
        SignatureToken::DatatypeInstantiation(idx, ty_args) => {
            format_signature_token_struct(view, *idx, ty_args)
        }
    }
}

pub fn format_signature_token_struct(
    view: &BinaryIndexedView,
    sidx: DatatypeHandleIndex,
    ty_args: &[SignatureToken],
) -> String {
    let (address, module_name, struct_name) = resolve_struct(view, sidx);
    let s;
    let ty_args_string = if ty_args.is_empty() {
        ""
    } else {
        s = format!(
            "<{}>",
            ty_args
                .iter()
                .map(|t| format_signature_token(view, t))
                .collect::<Vec<_>>()
                .join(", ")
        );
        &s
    };
    format!(
        "0x{}::{}::{}{}",
        address.short_str_lossless(),
        module_name,
        struct_name,
        ty_args_string
    )
}
