// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use super::{ast::DatatypeInfo, package_cache::PackageStorageId, BinaryCache};
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::SignatureToken,
    CompiledModule,
};
use move_core_types::{identifier::Identifier, vm_status::StatusCode};
use move_vm_types::loaded_data::runtime_types::{CachedDatatype, CachedTypeIndex, Type};

pub type DatatypeCacheIndex = u64;
pub type DatatypeKey = (PackageStorageId, Identifier, Identifier);
pub type DatatypeCache = BinaryCache<DatatypeKey, CachedDatatype>;

pub struct TypeCache {
    pub cached_types: DatatypeCache,
    pub cached_instantiations: HashMap<CachedTypeIndex, HashMap<Vec<Type>, DatatypeInfo>>,
}

impl TypeCache {
    pub(crate) fn new() -> Self {
        Self {
            cached_types: DatatypeCache::new(),
            cached_instantiations: HashMap::new(),
        }
    }

    pub fn cache_datatype(
        &mut self,
        key: DatatypeKey,
        datatype: CachedDatatype,
    ) -> PartialVMResult<&Arc<CachedDatatype>> {
        let _ = self.cached_types.insert(key.clone(), datatype);
        Ok(self.cached_types.get(&key).unwrap())
    }

    pub fn instantiate_type(
        &mut self,
        type_index: CachedTypeIndex,
        type_args: Vec<Type>,
        datatype: DatatypeInfo,
    ) -> PartialVMResult<&DatatypeInfo> {
        let instantiation_cache = self
            .cached_instantiations
            .entry(type_index)
            .or_insert_with(HashMap::new);
        instantiation_cache.insert(type_args.clone(), datatype);
        Ok(instantiation_cache.get(&type_args).unwrap())
    }

    pub fn resolve_type_by_name(
        &self,
        datatype_key: &DatatypeKey,
    ) -> PartialVMResult<(CachedTypeIndex, Arc<CachedDatatype>)> {
        match self.cached_types.get_with_idx(datatype_key) {
            Some((idx, datatype)) => Ok((CachedTypeIndex(idx), Arc::clone(datatype))),
            None => {
                println!("CACHE: {:#?}", self.cached_types.id_map);
                Err(dbg!(PartialVMError::new(
                    StatusCode::TYPE_RESOLUTION_FAILURE
                )
                .with_message(format!(
                    "Cannot find {}::{}::{} in cache",
                    datatype_key.0, datatype_key.1, datatype_key.2
                ))))
            }
        }
    }

    // `make_type` is the entry point to "translate" a `SignatureToken` to a `Type`
    pub(crate) fn make_type(
        &self,
        module: &CompiledModule,
        tok: &SignatureToken,
    ) -> PartialVMResult<Type> {
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
            SignatureToken::Vector(inner_tok) => {
                Type::Vector(Box::new(self.make_type(module, inner_tok)?))
            }
            SignatureToken::Reference(inner_tok) => {
                Type::Reference(Box::new(self.make_type(module, inner_tok)?))
            }
            SignatureToken::MutableReference(inner_tok) => {
                Type::MutableReference(Box::new(self.make_type(module, inner_tok)?))
            }
            SignatureToken::Datatype(sh_idx) => {
                let datatype_handle = module.datatype_handle_at(*sh_idx);
                let datatype_name = module.identifier_at(datatype_handle.name);
                let module_handle = module.module_handle_at(datatype_handle.module);
                // TODO/XXX: This does not handle upgrades properly yet.
                let runtime_address = module.address_identifier_at(module_handle.address);
                let module_name = module.identifier_at(module_handle.name).to_owned();
                let cache_idx = self
                    .resolve_type_by_name(&(
                        runtime_address.to_owned(),
                        module_name,
                        datatype_name.to_owned(),
                    ))?
                    .0;
                Type::Datatype(cache_idx)
            }
            SignatureToken::DatatypeInstantiation(inst) => {
                let (sh_idx, tys) = &**inst;
                let type_parameters: Vec<_> = tys
                    .iter()
                    .map(|tok| self.make_type(module, tok))
                    .collect::<PartialVMResult<_>>()?;
                let datatype_handle = module.datatype_handle_at(*sh_idx);
                let datatype_name = module.identifier_at(datatype_handle.name);
                let module_handle = module.module_handle_at(datatype_handle.module);
                // TODO/XXX: This does not handle upgrades properly yet.
                let runtime_address = module.address_identifier_at(module_handle.address);
                let module_name = module.identifier_at(module_handle.name).to_owned();
                let cache_idx = self
                    .resolve_type_by_name(&(
                        runtime_address.to_owned(),
                        module_name,
                        datatype_name.to_owned(),
                    ))?
                    .0;
                Type::DatatypeInstantiation(Box::new((cache_idx, type_parameters)))
            }
        };
        Ok(res)
    }
}
