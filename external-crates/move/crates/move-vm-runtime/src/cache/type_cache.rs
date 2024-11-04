// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::identifier_interner::IdentifierInterner,
    jit::execution::ast::{CachedDatatype, DatatypeInfo, IntraPackageKey, Type, VTableKey},
    shared::{
        binary_cache::BinaryCache, constants::MAX_TYPE_INSTANTIATION_NODES, types::RuntimePackageId,
    },
    string_interner,
};
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::SignatureToken,
    CompiledModule,
};
use move_core_types::vm_status::StatusCode;
use parking_lot::RwLock;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

#[derive(Debug)]
pub struct TypeCache {
    pub package_cache: BTreeMap<RuntimePackageId, Arc<RwLock<CrossVersionPackageCache>>>,
}

#[derive(Debug)]
pub struct CrossVersionPackageCache {
    pub package_uid: RuntimePackageId,
    pub cached_types: BinaryCache<IntraPackageKey, CachedDatatype>,
    pub cached_instantiations: HashMap<IntraPackageKey, HashMap<Vec<Type>, DatatypeInfo>>,
}

impl CrossVersionPackageCache {
    pub fn new(package_uid: RuntimePackageId) -> Self {
        Self {
            package_uid,
            cached_types: BinaryCache::new(),
            cached_instantiations: HashMap::new(),
        }
    }

    pub fn to_vtable_key(&self, key: &IntraPackageKey) -> VTableKey {
        VTableKey {
            package_key: self.package_uid,
            inner_pkg_key: key.clone(),
        }
    }

    pub fn cache_datatype(
        &mut self,
        key: IntraPackageKey,
        datatype: CachedDatatype,
    ) -> PartialVMResult<Arc<CachedDatatype>> {
        self.cached_types.insert(key, datatype).cloned()
    }

    pub fn instantiate_type(
        &mut self,
        string_interner: Arc<IdentifierInterner>,
        key: &IntraPackageKey,
        type_args: Vec<Type>,
        datatype: DatatypeInfo,
    ) -> PartialVMResult<&DatatypeInfo> {
        if self.cached_types.contains(key) {
            let module_name = string_interner.resolve_string(&key.module_name, "module name")?;
            let member_name = string_interner.resolve_string(&key.module_name, "member name")?;
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    format!(
                        "Type {}::{}::{} not found in type cache",
                        self.package_uid, module_name, member_name,
                    ),
                ),
            );
        }
        self.cached_instantiations
            .entry(key.clone())
            .or_default()
            .insert(type_args.clone(), datatype);
        Ok(self
            .cached_instantiations
            .get(key)
            .expect("Inserted idx above so key always exists")
            .get(&type_args)
            .expect("Inserted type_args above so key always exists"))
    }

    pub fn contains_cached_type(&self, key: &IntraPackageKey) -> bool {
        self.cached_types.contains(key)
    }

    pub fn resolve_type_by_name(
        &self,
        type_key: &IntraPackageKey,
    ) -> PartialVMResult<(VTableKey, Arc<CachedDatatype>)> {
        match self.cached_types.get(type_key) {
            Some(datatype) => Ok((self.to_vtable_key(type_key), Arc::clone(datatype))),
            None => {
                let module_name =
                    string_interner().resolve_string(&type_key.module_name, "module name")?;
                let member_name =
                    string_interner().resolve_string(&type_key.module_name, "member name")?;
                Err(
                    PartialVMError::new(StatusCode::TYPE_RESOLUTION_FAILURE).with_message(format!(
                        "Cannot find {}::{}::{} in cache",
                        self.package_uid, module_name, member_name,
                    )),
                )
            }
        }
    }

    pub fn type_at(&self, idx: &IntraPackageKey) -> Arc<CachedDatatype> {
        self.cached_types
            .get(idx)
            .expect("Type should exist")
            .clone()
    }
}

impl TypeCache {
    pub(crate) fn new() -> Self {
        Self {
            package_cache: BTreeMap::new(),
        }
    }

    pub(crate) fn get_or_create_package_cache(
        &mut self,
        package_key: RuntimePackageId,
    ) -> Arc<RwLock<CrossVersionPackageCache>> {
        self.package_cache
            .entry(package_key)
            .or_insert_with(|| Arc::new(RwLock::new(CrossVersionPackageCache::new(package_key))))
            .clone()
    }
}

pub fn make_type(module: &CompiledModule, tok: &SignatureToken) -> PartialVMResult<Type> {
    let res = match tok {
        SignatureToken::Bool => Type::Bool,
        SignatureToken::U8 => Type::U8,
        SignatureToken::U16 => Type::U16,
        SignatureToken::U32 => Type::U32,
        SignatureToken::U64 => Type::U64,
        SignatureToken::U128 => Type::U128,
        SignatureToken::U256 => Type::U256,
        SignatureToken::Address => Type::Address,
        SignatureToken::Signer => Type::Signer,
        SignatureToken::TypeParameter(idx) => Type::TyParam(*idx),
        SignatureToken::Vector(inner_tok) => Type::Vector(Box::new(make_type(module, inner_tok)?)),
        SignatureToken::Reference(inner_tok) => {
            Type::Reference(Box::new(make_type(module, inner_tok)?))
        }
        SignatureToken::MutableReference(inner_tok) => {
            Type::MutableReference(Box::new(make_type(module, inner_tok)?))
        }
        SignatureToken::Datatype(sh_idx) => {
            let datatype_handle = module.datatype_handle_at(*sh_idx);
            let datatype_name = string_interner()
                .get_or_intern_ident_str(module.identifier_at(datatype_handle.name))?;
            let module_handle = module.module_handle_at(datatype_handle.module);
            let runtime_address = module.address_identifier_at(module_handle.address);
            let module_name = string_interner()
                .get_or_intern_ident_str(module.identifier_at(module_handle.name))?;
            let cache_idx = VTableKey {
                package_key: *runtime_address,
                inner_pkg_key: IntraPackageKey {
                    module_name,
                    member_name: datatype_name.to_owned(),
                },
            };
            Type::Datatype(cache_idx)
        }
        SignatureToken::DatatypeInstantiation(inst) => {
            let (sh_idx, tys) = &**inst;
            let type_parameters: Vec<_> = tys
                .iter()
                .map(|tok| make_type(module, tok))
                .collect::<PartialVMResult<_>>()?;
            let datatype_handle = module.datatype_handle_at(*sh_idx);
            let datatype_name = string_interner()
                .get_or_intern_ident_str(module.identifier_at(datatype_handle.name))?;
            let module_handle = module.module_handle_at(datatype_handle.module);
            let runtime_address = module.address_identifier_at(module_handle.address);
            let module_name = string_interner()
                .get_or_intern_ident_str(module.identifier_at(module_handle.name))?;
            let cache_idx = VTableKey {
                package_key: *runtime_address,
                inner_pkg_key: IntraPackageKey {
                    module_name,
                    member_name: datatype_name.to_owned(),
                },
            };
            Type::DatatypeInstantiation(Box::new((cache_idx, type_parameters)))
        }
    };
    Ok(res)
}

// -------------------------------------------------------------------------------------------------
// Helper Functions
// -------------------------------------------------------------------------------------------------

// Return an instantiated type given a generic and an instantiation.
// Stopgap to avoid a recursion that is either taking too long or using too
// much memory
pub fn subst(ty: &Type, ty_args: &[Type]) -> PartialVMResult<Type> {
    // Before instantiating the type, count the # of nodes of all type arguments plus
    // existing type instantiation.
    // If that number is larger than MAX_TYPE_INSTANTIATION_NODES, refuse to construct this type.
    // This prevents constructing larger and larger types via datatype instantiation.
    if let Type::DatatypeInstantiation(inst) = ty {
        let (_, datatype_inst) = &**inst;
        let mut sum_nodes = 1u64;
        for ty in ty_args.iter().chain(datatype_inst.iter()) {
            sum_nodes = sum_nodes.saturating_add(count_type_nodes(ty));
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
            }
        }
    }
    ty.subst(ty_args)
}

pub fn count_type_nodes(ty: &Type) -> u64 {
    let mut todo = vec![ty];
    let mut result = 0;
    while let Some(ty) = todo.pop() {
        match ty {
            Type::Vector(ty) | Type::Reference(ty) | Type::MutableReference(ty) => {
                result += 1;
                todo.push(ty);
            }
            Type::DatatypeInstantiation(struct_inst) => {
                let (_, ty_args) = &**struct_inst;
                result += 1;
                todo.extend(ty_args.iter())
            }
            _ => {
                result += 1;
            }
        }
    }
    result
}
