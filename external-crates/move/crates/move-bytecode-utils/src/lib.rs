// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod layout;
pub mod module_cache;

use move_binary_format::file_format::{CompiledModule, DatatypeHandleIndex, SignatureToken};
use move_core_types::{
    account_address::AccountAddress, identifier::IdentStr, language_storage::ModuleId,
};

use anyhow::{anyhow, Result};
use indexmap::IndexMap;
use petgraph::graphmap::DiGraphMap;
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
            if let Some(prev) = map.insert(m.self_id(), m) {
                panic!(
                    "Duplicate module found: {}",
                    prev.self_id().to_canonical_display(/* with_prefix */ true)
                )
            }
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

    /// Return an iterator over the modules in `self` in topological order--modules with least deps first.
    /// Fails with an error if `self` contains circular dependencies.
    /// Tolerates missing dependencies.
    pub fn compute_topological_order(&self) -> Result<impl Iterator<Item = &CompiledModule>> {
        let mut module_map = IndexMap::new();
        for m in self.iter_modules() {
            if module_map.insert(m.self_id(), m).is_some() {
                panic!("Duplicate module found")
            }
        }

        let mut graph: DiGraphMap<usize, usize> = DiGraphMap::new();
        for i in 0..module_map.len() {
            graph.add_node(i);
        }

        for (i, (_, m)) in module_map.iter().enumerate() {
            for dep in m.immediate_dependencies() {
                if let Some(j) = module_map.get_index_of(&dep) {
                    graph.add_edge(i, j, 0);
                }
            }
        }

        match petgraph::algo::toposort(&graph, None) {
            Err(_) => panic!("Circular dependency detected"),
            Ok(ordered_idxs) => Ok(ordered_idxs
                .into_iter()
                .map(move |idx| *module_map.get_index(idx).unwrap().1)
                .rev()),
        }
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

pub fn resolve_struct(
    module: &CompiledModule,
    sidx: DatatypeHandleIndex,
) -> (&AccountAddress, &IdentStr, &IdentStr) {
    let shandle = module.datatype_handle_at(sidx);
    let mhandle = module.module_handle_at(shandle.module);
    let address = module.address_identifier_at(mhandle.address);
    let module_name = module.identifier_at(mhandle.name);
    let struct_name = module.identifier_at(shandle.name);
    (address, module_name, struct_name)
}

pub fn format_signature_token(module: &CompiledModule, t: &SignatureToken) -> String {
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
            format!("vector<{}>", format_signature_token(module, inner))
        }
        SignatureToken::Reference(inner) => format!("&{}", format_signature_token(module, inner)),
        SignatureToken::MutableReference(inner) => {
            format!("&mut {}", format_signature_token(module, inner))
        }
        SignatureToken::TypeParameter(i) => format!("T{}", i),

        SignatureToken::Datatype(idx) => format_signature_token_struct(module, *idx, &[]),
        SignatureToken::DatatypeInstantiation(inst) => {
            let (idx, ty_args) = &**inst;
            format_signature_token_struct(module, *idx, ty_args)
        }
    }
}

pub fn format_signature_token_struct(
    module: &CompiledModule,
    sidx: DatatypeHandleIndex,
    ty_args: &[SignatureToken],
) -> String {
    let (address, module_name, struct_name) = resolve_struct(module, sidx);
    let s;
    let ty_args_string = if ty_args.is_empty() {
        ""
    } else {
        s = format!(
            "<{}>",
            ty_args
                .iter()
                .map(|t| format_signature_token(module, t))
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
