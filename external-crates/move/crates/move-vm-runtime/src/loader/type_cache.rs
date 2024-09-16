// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::{
    ast::{DatatypeInfo, DefiningTypeId},
    BinaryCache,
};
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{SignatureToken, TypeParameterIndex},
    CompiledModule,
};
use move_core_types::{identifier::Identifier, language_storage::ModuleId, vm_status::StatusCode};
use move_vm_types::{
    data_store::DataStore,
    loaded_data::runtime_types::{CachedDatatype, CachedTypeIndex, Datatype, DepthFormula, Type},
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

pub type DatatypeCacheIndex = u64;
pub type DatatypeKey = (DefiningTypeId, Identifier, Identifier);
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
        let instantiation_cache = self.cached_instantiations.entry(type_index).or_default();
        instantiation_cache.insert(type_args.clone(), datatype);
        Ok(instantiation_cache.get(&type_args).unwrap())
    }

    pub fn type_at(&self, idx: CachedTypeIndex) -> Arc<CachedDatatype> {
        self.cached_types.binaries[idx.0].clone()
    }

    pub fn mut_type_at(&mut self, idx: CachedTypeIndex) -> &mut Arc<CachedDatatype> {
        &mut self.cached_types.binaries[idx.0]
    }

    pub fn resolve_type_by_name(
        &self,
        datatype_key: &DatatypeKey,
    ) -> PartialVMResult<(CachedTypeIndex, Arc<CachedDatatype>)> {
        match self.cached_types.get_with_idx(datatype_key) {
            Some((idx, datatype)) => Ok((CachedTypeIndex(idx), Arc::clone(datatype))),
            None => Err(
                PartialVMError::new(StatusCode::TYPE_RESOLUTION_FAILURE).with_message(format!(
                    "Cannot find {}::{}::{} in cache",
                    datatype_key.0, datatype_key.1, datatype_key.2
                )),
            ),
        }
    }

    // `make_type` is the entry point to "translate" a `SignatureToken` to a `Type`
    pub(crate) fn make_type(
        &self,
        module: &CompiledModule,
        tok: &SignatureToken,
        data_store: &impl DataStore,
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
                Type::Vector(Box::new(self.make_type(module, inner_tok, data_store)?))
            }
            SignatureToken::Reference(inner_tok) => {
                Type::Reference(Box::new(self.make_type(module, inner_tok, data_store)?))
            }
            SignatureToken::MutableReference(inner_tok) => {
                Type::MutableReference(Box::new(self.make_type(module, inner_tok, data_store)?))
            }
            SignatureToken::Datatype(sh_idx) => {
                let datatype_handle = module.datatype_handle_at(*sh_idx);
                let datatype_name = module.identifier_at(datatype_handle.name);
                let module_handle = module.module_handle_at(datatype_handle.module);
                let runtime_address = module.address_identifier_at(module_handle.address);
                let module_name = module.identifier_at(module_handle.name).to_owned();
                let defining_type_id = data_store.defining_module(
                    &ModuleId::new(*runtime_address, module_name.clone()),
                    datatype_name,
                )?;
                let cache_idx = self
                    .resolve_type_by_name(&(
                        *defining_type_id.address(),
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
                    .map(|tok| self.make_type(module, tok, data_store))
                    .collect::<PartialVMResult<_>>()?;
                let datatype_handle = module.datatype_handle_at(*sh_idx);
                let datatype_name = module.identifier_at(datatype_handle.name);
                let module_handle = module.module_handle_at(datatype_handle.module);
                let runtime_address = module.address_identifier_at(module_handle.address);
                let module_name = module.identifier_at(module_handle.name).to_owned();
                let defining_type_id = data_store.defining_module(
                    &ModuleId::new(*runtime_address, module_name.clone()),
                    datatype_name,
                )?;
                let cache_idx = self
                    .resolve_type_by_name(&(
                        *defining_type_id.address(),
                        module_name,
                        datatype_name.to_owned(),
                    ))?
                    .0;
                Type::DatatypeInstantiation(Box::new((cache_idx, type_parameters)))
            }
        };
        Ok(res)
    }

    pub(crate) fn calculate_depth_of_datatype(
        &self,
        datatype: &CachedDatatype,
        depth_cache: &mut BTreeMap<CachedTypeIndex, DepthFormula>,
    ) -> PartialVMResult<DepthFormula> {
        let def_idx = self.resolve_type_by_name(&datatype.datatype_key())?.0;

        // If we've already computed this datatypes depth, no more work remains to be done.
        if let Some(form) = &datatype.depth {
            return Ok(form.clone());
        }
        if let Some(form) = depth_cache.get(&def_idx) {
            return Ok(form.clone());
        }

        let formulas = match &datatype.datatype_info {
            // The depth of enum is calculated as the maximum depth of any of its variants.
            Datatype::Enum(enum_type) => enum_type
                .variants
                .iter()
                .flat_map(|variant_type| variant_type.fields.iter())
                .map(|field_type| self.calculate_depth_of_type(field_type, depth_cache))
                .collect::<PartialVMResult<Vec<_>>>()?,
            Datatype::Struct(struct_type) => struct_type
                .fields
                .iter()
                .map(|field_type| self.calculate_depth_of_type(field_type, depth_cache))
                .collect::<PartialVMResult<Vec<_>>>()?,
        };
        let mut formula = DepthFormula::normalize(formulas);
        // add 1 for the struct/variant itself
        formula.add(1);
        let prev = depth_cache.insert(def_idx, formula.clone());
        if prev.is_some() {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Recursive type?".to_owned()),
            );
        }
        Ok(formula)
    }

    fn calculate_depth_of_type(
        &self,
        ty: &Type,
        depth_cache: &mut BTreeMap<CachedTypeIndex, DepthFormula>,
    ) -> PartialVMResult<DepthFormula> {
        Ok(match ty {
            Type::Bool
            | Type::U8
            | Type::U64
            | Type::U128
            | Type::Address
            | Type::Signer
            | Type::U16
            | Type::U32
            | Type::U256 => DepthFormula::constant(1),
            // we should not see the reference here, we could instead give an invariant violation
            Type::Vector(ty) | Type::Reference(ty) | Type::MutableReference(ty) => {
                let mut inner = self.calculate_depth_of_type(ty, depth_cache)?;
                // add 1 for the vector itself
                inner.add(1);
                inner
            }
            Type::TyParam(ty_idx) => DepthFormula::type_parameter(*ty_idx),
            Type::Datatype(cache_idx) => {
                let datatype = self.type_at(*cache_idx);
                let datatype_formula = self.calculate_depth_of_datatype(&datatype, depth_cache)?;
                debug_assert!(datatype_formula.terms.is_empty());
                datatype_formula
            }
            Type::DatatypeInstantiation(inst) => {
                let (cache_idx, ty_args) = &**inst;
                let datatype = self.type_at(*cache_idx);
                let ty_arg_map = ty_args
                    .iter()
                    .enumerate()
                    .map(|(idx, ty)| {
                        let var = idx as TypeParameterIndex;
                        Ok((var, self.calculate_depth_of_type(ty, depth_cache)?))
                    })
                    .collect::<PartialVMResult<BTreeMap<_, _>>>()?;
                let datatype_formula = self.calculate_depth_of_datatype(&datatype, depth_cache)?;

                datatype_formula.subst(ty_arg_map)?
            }
        })
    }
}
