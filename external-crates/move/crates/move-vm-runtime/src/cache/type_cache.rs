// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    jit::runtime::ast::{DatatypeInfo, DatatypeTagType},
    on_chain::ast::DefiningTypeId,
    shared::{
        binary_cache::BinaryCache,
        constants::{MAX_TYPE_INSTANTIATION_NODES, MAX_TYPE_TO_LAYOUT_NODES, VALUE_DEPTH_MAX},
    },
};
use move_binary_format::{
    errors::{Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{AbilitySet, SignatureToken, TypeParameterIndex},
    CompiledModule,
};
use move_core_types::{
    annotated_value,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
    runtime_value,
    vm_status::StatusCode,
};
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

// TODO: Think about how we want to handle parallelization and scratch/transaction-local types
// here. Also worth thinking about type layout differences(?) between package types.
#[derive(Debug)]
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

    pub fn type_at(&self, idx: CachedTypeIndex) -> Arc<CachedDatatype> {
        self.cached_types.binaries[idx.0].clone()
    }

    pub fn mut_type_at(&mut self, idx: CachedTypeIndex) -> &mut Arc<CachedDatatype> {
        &mut self.cached_types.binaries[idx.0]
    }

    // TODO: calls to this are dubious. How did you get a cached type index without it being a
    // valid cached datatype?
    pub fn get_type(&self, idx: CachedTypeIndex) -> PartialVMResult<Arc<CachedDatatype>> {
        self.cached_types
            .binaries
            .get(idx.0)
            .map(|value: &Arc<CachedDatatype>| value.clone())
            .ok_or(PartialVMError::new(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            ))
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

    // -------------------------------------------
    // Helpers for loading and verification
    // -------------------------------------------

    // `make_type` is the entry point to "translate" a `SignatureToken` to a `Type`
    pub(crate) fn make_type(
        &self,
        module: &CompiledModule,
        tok: &SignatureToken,
        data_store: &dyn DataStore,
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
                    &datatype_name,
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
                    &datatype_name,
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

    fn load_type_by_name(
        &self,
        datatype_name: &IdentStr,
        runtime_id: &ModuleId,
        data_store: &dyn DataStore,
    ) -> PartialVMResult<(CachedTypeIndex, Arc<CachedDatatype>)> {
        let defining_id = data_store.defining_module(runtime_id, datatype_name)?;
        self.resolve_type_by_name(&(
            *defining_id.address(),
            runtime_id.name().to_owned(),
            datatype_name.to_owned(),
        ))
    }

    pub(crate) fn load_type(
        &self,
        type_tag: &TypeTag,
        data_store: &dyn DataStore,
    ) -> VMResult<Type> {
        Ok(match type_tag {
            TypeTag::Bool => Type::Bool,
            TypeTag::U8 => Type::U8,
            TypeTag::U16 => Type::U16,
            TypeTag::U32 => Type::U32,
            TypeTag::U64 => Type::U64,
            TypeTag::U128 => Type::U128,
            TypeTag::U256 => Type::U256,
            TypeTag::Address => Type::Address,
            TypeTag::Signer => Type::Signer,
            TypeTag::Vector(tt) => Type::Vector(Box::new(self.load_type(tt, data_store)?)),
            TypeTag::Struct(struct_tag) => {
                let runtime_id = ModuleId::new(struct_tag.address, struct_tag.module.clone());
                let (idx, struct_type) = self
                    .load_type_by_name(&struct_tag.name, &runtime_id, data_store)
                    .map_err(|e| e.finish(Location::Undefined))?;
                if struct_type.type_parameters.is_empty() && struct_tag.type_params.is_empty() {
                    Type::Datatype(idx)
                } else {
                    let mut type_params = vec![];
                    for ty_param in &struct_tag.type_params {
                        type_params.push(self.load_type(ty_param, data_store)?);
                    }
                    self.verify_ty_args(struct_type.type_param_constraints(), &type_params)
                        .map_err(|e| e.finish(Location::Undefined))?;
                    Type::DatatypeInstantiation(Box::new((idx, type_params)))
                }
            }
        })
    }

    // Verify the kind (constraints) of an instantiation.
    // Function invocations call this function to verify correctness of type arguments provided
    pub fn verify_ty_args<'a, I>(&self, constraints: I, ty_args: &[Type]) -> PartialVMResult<()>
    where
        I: IntoIterator<Item = &'a AbilitySet>,
        I::IntoIter: ExactSizeIterator,
    {
        let constraints = constraints.into_iter();
        if constraints.len() != ty_args.len() {
            return Err(PartialVMError::new(
                StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH,
            ));
        }
        for (ty, expected_k) in ty_args.iter().zip(constraints) {
            if !expected_k.is_subset(self.abilities(ty)?) {
                return Err(PartialVMError::new(StatusCode::CONSTRAINT_NOT_SATISFIED));
            }
        }
        Ok(())
    }

    pub(crate) fn abilities(&self, ty: &Type) -> PartialVMResult<AbilitySet> {
        match ty {
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::U256
            | Type::Address => Ok(AbilitySet::PRIMITIVES),

            // Technically unreachable but, no point in erroring if we don't have to
            Type::Reference(_) | Type::MutableReference(_) => Ok(AbilitySet::REFERENCES),
            Type::Signer => Ok(AbilitySet::SIGNER),

            Type::TyParam(_) => Err(PartialVMError::new(StatusCode::UNREACHABLE).with_message(
                "Unexpected TyParam type after translating from TypeTag to Type".to_string(),
            )),

            Type::Vector(ty) => AbilitySet::polymorphic_abilities(
                AbilitySet::VECTOR,
                vec![false],
                vec![self.abilities(ty)?],
            ),
            Type::Datatype(idx) => Ok(self.type_at(*idx).abilities),
            Type::DatatypeInstantiation(inst) => {
                let (idx, type_args) = &**inst;
                let datatype_type = self.type_at(*idx);
                let declared_phantom_parameters = datatype_type
                    .type_parameters
                    .iter()
                    .map(|param| param.is_phantom);
                let type_argument_abilities = type_args
                    .iter()
                    .map(|arg| self.abilities(arg))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                AbilitySet::polymorphic_abilities(
                    datatype_type.abilities,
                    declared_phantom_parameters,
                    type_argument_abilities,
                )
            }
        }
    }

    // -------------------------------------------
    // Type Depth Computations
    // -------------------------------------------

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

    // -------------------------------------------
    // Type Translation Helpers
    // -------------------------------------------

    fn read_cached_struct_tag(
        &self,
        gidx: CachedTypeIndex,
        ty_args: &[Type],
        tag_type: DatatypeTagType,
    ) -> Option<StructTag> {
        let map = self.cached_instantiations.get(&gidx)?;
        let info = map.get(ty_args)?;

        match tag_type {
            DatatypeTagType::Runtime => info.runtime_tag.clone(),
            DatatypeTagType::Defining => info.defining_tag.clone(),
        }
    }

    fn datatype_gidx_to_type_tag(
        &mut self,
        gidx: CachedTypeIndex,
        ty_args: &[Type],
        tag_type: DatatypeTagType,
    ) -> PartialVMResult<StructTag> {
        if let Some(cached) = self.read_cached_struct_tag(gidx, ty_args, tag_type) {
            return Ok(cached);
        }

        let ty_arg_tags = ty_args
            .iter()
            .map(|ty| self.type_to_type_tag_impl(ty, tag_type))
            .collect::<PartialVMResult<Vec<_>>>()?;
        let datatype = self.type_at(gidx);

        let info = self
            .cached_instantiations
            .entry(gidx)
            .or_default()
            .entry(ty_args.to_vec())
            .or_insert_with(DatatypeInfo::new);

        match tag_type {
            DatatypeTagType::Runtime => {
                let tag = StructTag {
                    address: *datatype.runtime_id.address(),
                    module: datatype.runtime_id.name().to_owned(),
                    name: datatype.name.clone(),
                    type_params: ty_arg_tags,
                };

                info.runtime_tag = Some(tag.clone());
                Ok(tag)
            }

            DatatypeTagType::Defining => {
                let tag = StructTag {
                    address: *datatype.defining_id.address(),
                    module: datatype.defining_id.name().to_owned(),
                    name: datatype.name.clone(),
                    type_params: ty_arg_tags,
                };

                info.defining_tag = Some(tag.clone());
                Ok(tag)
            }
        }
    }

    fn type_to_type_tag_impl(
        &mut self,
        ty: &Type,
        tag_type: DatatypeTagType,
    ) -> PartialVMResult<TypeTag> {
        Ok(match ty {
            Type::Bool => TypeTag::Bool,
            Type::U8 => TypeTag::U8,
            Type::U16 => TypeTag::U16,
            Type::U32 => TypeTag::U32,
            Type::U64 => TypeTag::U64,
            Type::U128 => TypeTag::U128,
            Type::U256 => TypeTag::U256,
            Type::Address => TypeTag::Address,
            Type::Signer => TypeTag::Signer,
            Type::Vector(ty) => {
                TypeTag::Vector(Box::new(self.type_to_type_tag_impl(ty, tag_type)?))
            }
            Type::Datatype(gidx) => TypeTag::Struct(Box::new(self.datatype_gidx_to_type_tag(
                *gidx,
                &[],
                tag_type,
            )?)),
            Type::DatatypeInstantiation(struct_inst) => {
                let (gidx, ty_args) = &**struct_inst;
                TypeTag::Struct(Box::new(
                    self.datatype_gidx_to_type_tag(*gidx, ty_args, tag_type)?,
                ))
            }
            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("no type tag for {:?}", ty)),
                );
            }
        })
    }

    fn type_gidx_to_type_layout(
        &mut self,
        gidx: CachedTypeIndex,
        ty_args: &[Type],
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<runtime_value::MoveDatatypeLayout> {
        if let Some(type_map) = self.cached_instantiations.get(&gidx) {
            if let Some(type_info) = type_map.get(ty_args) {
                if let Some(node_count) = &type_info.node_count {
                    *count += *node_count
                }
                if let Some(layout) = &type_info.layout {
                    return Ok(layout.clone());
                }
            }
        }

        let count_before = *count;
        let ty = self.type_at(gidx);
        let type_layout = match ty.datatype_info {
            Datatype::Enum(ref einfo) => {
                let mut variant_layouts = vec![];
                for variant in einfo.variants.iter() {
                    let field_tys = variant
                        .fields
                        .iter()
                        .map(|ty| subst(ty, ty_args))
                        .collect::<PartialVMResult<Vec<_>>>()?;
                    let field_layouts = field_tys
                        .iter()
                        .map(|ty| self.type_to_type_layout_impl(ty, count, depth + 1))
                        .collect::<PartialVMResult<Vec<_>>>()?;
                    variant_layouts.push(field_layouts);
                }
                runtime_value::MoveDatatypeLayout::Enum(runtime_value::MoveEnumLayout(
                    variant_layouts,
                ))
            }
            Datatype::Struct(ref sinfo) => {
                let field_tys = sinfo
                    .fields
                    .iter()
                    .map(|ty| subst(ty, ty_args))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let field_layouts = field_tys
                    .iter()
                    .map(|ty| self.type_to_type_layout_impl(ty, count, depth + 1))
                    .collect::<PartialVMResult<Vec<_>>>()?;

                runtime_value::MoveDatatypeLayout::Struct(runtime_value::MoveStructLayout::new(
                    field_layouts,
                ))
            }
        };

        let field_node_count = *count - count_before;

        let info = self
            .cached_instantiations
            .entry(gidx)
            .or_default()
            .entry(ty_args.to_vec())
            .or_insert_with(DatatypeInfo::new);
        info.layout = Some(type_layout.clone());
        info.node_count = Some(field_node_count);

        Ok(type_layout)
    }

    fn type_to_type_layout_impl(
        &mut self,
        ty: &Type,
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<runtime_value::MoveTypeLayout> {
        if *count > MAX_TYPE_TO_LAYOUT_NODES {
            return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
        }
        if depth > VALUE_DEPTH_MAX {
            return Err(PartialVMError::new(StatusCode::VM_MAX_VALUE_DEPTH_REACHED));
        }
        *count += 1;
        Ok(match ty {
            Type::Bool => runtime_value::MoveTypeLayout::Bool,
            Type::U8 => runtime_value::MoveTypeLayout::U8,
            Type::U16 => runtime_value::MoveTypeLayout::U16,
            Type::U32 => runtime_value::MoveTypeLayout::U32,
            Type::U64 => runtime_value::MoveTypeLayout::U64,
            Type::U128 => runtime_value::MoveTypeLayout::U128,
            Type::U256 => runtime_value::MoveTypeLayout::U256,
            Type::Address => runtime_value::MoveTypeLayout::Address,
            Type::Signer => runtime_value::MoveTypeLayout::Signer,
            Type::Vector(ty) => runtime_value::MoveTypeLayout::Vector(Box::new(
                self.type_to_type_layout_impl(ty, count, depth + 1)?,
            )),
            Type::Datatype(gidx) => self
                .type_gidx_to_type_layout(*gidx, &[], count, depth)?
                .into_layout(),
            Type::DatatypeInstantiation(inst) => {
                let (gidx, ty_args) = &**inst;
                self.type_gidx_to_type_layout(*gidx, ty_args, count, depth)?
                    .into_layout()
            }
            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("no type layout for {:?}", ty)),
                );
            }
        })
    }

    fn datatype_gidx_to_fully_annotated_layout(
        &mut self,
        gidx: CachedTypeIndex,
        ty_args: &[Type],
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<annotated_value::MoveDatatypeLayout> {
        if let Some(datatype_map) = self.cached_instantiations.get(&gidx) {
            if let Some(datatype_info) = datatype_map.get(ty_args) {
                if let Some(annotated_node_count) = &datatype_info.annotated_node_count {
                    *count += *annotated_node_count
                }
                if let Some(layout) = &datatype_info.annotated_layout {
                    return Ok(layout.clone());
                }
            }
        }

        let count_before = *count;
        let ty = self.type_at(gidx);
        let struct_tag =
            self.datatype_gidx_to_type_tag(gidx, ty_args, DatatypeTagType::Defining)?;
        let type_layout = match &ty.datatype_info {
            Datatype::Enum(enum_type) => {
                let mut variant_layouts = BTreeMap::new();
                for variant in enum_type.variants.iter() {
                    if variant.fields.len() != variant.field_names.len() {
                        return Err(
                            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                                "Field types did not match the length of field names in loaded enum variant"
                                .to_owned(),
                            ),
                        );
                    }
                    let field_layouts = variant
                        .field_names
                        .iter()
                        .zip(variant.fields.iter())
                        .map(|(n, ty)| {
                            let ty = subst(ty, ty_args)?;
                            let l =
                                self.type_to_fully_annotated_layout_impl(&ty, count, depth + 1)?;
                            Ok(annotated_value::MoveFieldLayout::new(n.clone(), l))
                        })
                        .collect::<PartialVMResult<Vec<_>>>()?;
                    variant_layouts.insert(
                        (variant.variant_name.clone(), variant.variant_tag),
                        field_layouts,
                    );
                }
                annotated_value::MoveDatatypeLayout::Enum(annotated_value::MoveEnumLayout {
                    type_: struct_tag.clone(),
                    variants: variant_layouts,
                })
            }
            Datatype::Struct(struct_type) => {
                if struct_type.fields.len() != struct_type.field_names.len() {
                    return Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message(
                            "Field types did not match the length of field names in loaded struct"
                                .to_owned(),
                        ),
                    );
                }
                let field_layouts = struct_type
                    .field_names
                    .iter()
                    .zip(&struct_type.fields)
                    .map(|(n, ty)| {
                        let ty = subst(ty, ty_args)?;
                        let l = self.type_to_fully_annotated_layout_impl(&ty, count, depth + 1)?;
                        Ok(annotated_value::MoveFieldLayout::new(n.clone(), l))
                    })
                    .collect::<PartialVMResult<Vec<_>>>()?;
                annotated_value::MoveDatatypeLayout::Struct(annotated_value::MoveStructLayout::new(
                    struct_tag,
                    field_layouts,
                ))
            }
        };

        let field_node_count = *count - count_before;

        let info = self
            .cached_instantiations
            .entry(gidx)
            .or_default()
            .entry(ty_args.to_vec())
            .or_insert_with(DatatypeInfo::new);
        info.annotated_layout = Some(type_layout.clone());
        info.annotated_node_count = Some(field_node_count);

        Ok(type_layout)
    }

    fn type_to_fully_annotated_layout_impl(
        &mut self,
        ty: &Type,
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<annotated_value::MoveTypeLayout> {
        if *count > MAX_TYPE_TO_LAYOUT_NODES {
            return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
        }
        if depth > VALUE_DEPTH_MAX {
            return Err(PartialVMError::new(StatusCode::VM_MAX_VALUE_DEPTH_REACHED));
        }
        *count += 1;
        Ok(match ty {
            Type::Bool => annotated_value::MoveTypeLayout::Bool,
            Type::U8 => annotated_value::MoveTypeLayout::U8,
            Type::U16 => annotated_value::MoveTypeLayout::U16,
            Type::U32 => annotated_value::MoveTypeLayout::U32,
            Type::U64 => annotated_value::MoveTypeLayout::U64,
            Type::U128 => annotated_value::MoveTypeLayout::U128,
            Type::U256 => annotated_value::MoveTypeLayout::U256,
            Type::Address => annotated_value::MoveTypeLayout::Address,
            Type::Signer => annotated_value::MoveTypeLayout::Signer,
            Type::Vector(ty) => annotated_value::MoveTypeLayout::Vector(Box::new(
                self.type_to_fully_annotated_layout_impl(ty, count, depth + 1)?,
            )),
            Type::Datatype(gidx) => self
                .datatype_gidx_to_fully_annotated_layout(*gidx, &[], count, depth)?
                .into_layout(),
            Type::DatatypeInstantiation(inst) => {
                let (gidx, ty_args) = &**inst;
                self.datatype_gidx_to_fully_annotated_layout(*gidx, ty_args, count, depth)?
                    .into_layout()
            }
            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("no type layout for {:?}", ty)),
                );
            }
        })
    }

    pub(crate) fn type_to_type_tag(&mut self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_to_type_tag_impl(ty, DatatypeTagType::Defining)
    }

    pub(crate) fn type_to_runtime_type_tag(&mut self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_to_type_tag_impl(ty, DatatypeTagType::Runtime)
    }

    pub(crate) fn type_to_type_layout(
        &mut self,
        ty: &Type,
    ) -> PartialVMResult<runtime_value::MoveTypeLayout> {
        let mut count = 0;
        self.type_to_type_layout_impl(ty, &mut count, 1)
    }

    pub(crate) fn type_to_fully_annotated_layout(
        &mut self,
        ty: &Type,
    ) -> PartialVMResult<annotated_value::MoveTypeLayout> {
        let mut count = 0;
        self.type_to_fully_annotated_layout_impl(ty, &mut count, 1)
    }

    // -------------------------------------------
    // Public APIs for type layout retrieval.
    // -------------------------------------------

    pub(crate) fn get_type_layout(
        &mut self,
        type_tag: &TypeTag,
        move_storage: &dyn DataStore,
    ) -> VMResult<runtime_value::MoveTypeLayout> {
        let ty = self.load_type(type_tag, move_storage)?;
        self.type_to_type_layout(&ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    pub(crate) fn get_fully_annotated_type_layout(
        &mut self,
        type_tag: &TypeTag,
        move_storage: &dyn DataStore,
    ) -> VMResult<annotated_value::MoveTypeLayout> {
        let ty = self.load_type(type_tag, move_storage)?;
        self.type_to_fully_annotated_layout(&ty)
            .map_err(|e| e.finish(Location::Undefined))
    }
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
