// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This module is responsible for the building of the package VTables given a root package storage
// ID. The VTables are built by loading all the packages that are dependencies of the root package,
// and once they are loaded creating the VTables for each package, and populating the
// `loaded_packages` table (keyed by the _runtime_ package ID!) with the VTables for each package
// in the transitive closure of the root package.

use crate::{
    cache::{arena::ArenaPointer, type_cache},
    jit::execution::ast::{
        CachedDatatype, Datatype, DatatypeTagType, DepthFormula, Function, IntraPackageKey, Module,
        Package, Type, VTableKey,
    },
    shared::{
        constants::{MAX_TYPE_TO_LAYOUT_NODES, VALUE_DEPTH_MAX},
        types::RuntimePackageId,
    },
    string_interner,
};
use move_binary_format::{
    errors::{Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{AbilitySet, TypeParameterIndex},
};
use move_core_types::{
    annotated_value,
    language_storage::{ModuleId, StructTag, TypeTag},
    runtime_value,
    vm_status::StatusCode,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tracing::error;

/// The data structure that the VM uses to resolve all packages. Packages are loaded into this at
/// before the beginning of execution, and based on the static call graph of the package that
/// contains the root package id.
///
/// This is a transient (transaction-scoped) data structure that is created at the beginning of the
/// transaction, is immutable for the execution of the transaction, and is dropped at the end of
/// the transaction.
///
/// TODO(tzakian): The representation can be optimized to use a more efficient data structure for
/// vtable/cross-package function resolution but we will keep it simple for now.
#[derive(Clone)]
pub struct VMDispatchTables {
    pub(crate) loaded_packages: HashMap<RuntimePackageId, Arc<Package>>,
}

/// The VM API that it will use to resolve packages and functions during execution of the
/// transaction.
impl VMDispatchTables {
    /// Create a new RuntimeVTables instance.
    /// NOTE: This assumes linkage has already occured.
    pub fn new(loaded_packages: HashMap<RuntimePackageId, Arc<Package>>) -> VMResult<Self> {
        Ok(Self { loaded_packages })
    }

    pub fn get_package(&self, id: &RuntimePackageId) -> PartialVMResult<Arc<Package>> {
        self.loaded_packages.get(id).cloned().ok_or_else(|| {
            PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                .with_message(format!("Package {} not found", id))
        })
    }

    pub fn resolve_loaded_module(&self, runtime_id: &ModuleId) -> PartialVMResult<Arc<Module>> {
        let (package, module_id) = runtime_id.into();
        let package = self.loaded_packages.get(package).ok_or_else(|| {
            PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                .with_message(format!("Package {} not found", package))
        })?;
        package
            .loaded_modules
            .get(module_id)
            .cloned()
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                    .with_message(format!("Module {} not found", module_id))
            })
    }

    pub fn resolve_function(
        &self,
        vtable_key: &VTableKey,
    ) -> PartialVMResult<ArenaPointer<Function>> {
        let Some(result) = self
            .loaded_packages
            .get(&vtable_key.package_key)
            .map(|pkg| &pkg.vtable)
            .and_then(|vtable| vtable.functions.get(&vtable_key.inner_pkg_key))
            .map(|f| *f.as_ref())
        else {
            let string_interner = string_interner();
            let module_name = string_interner
                .resolve_string(&vtable_key.inner_pkg_key.module_name, "module name")?;
            let member_name = string_interner
                .resolve_string(&vtable_key.inner_pkg_key.member_name, "member name")?;
            return Err(
                PartialVMError::new(StatusCode::MISSING_DEPENDENCY).with_message(format!(
                    "Function {module_name}::{member_name} not found in package {}",
                    vtable_key.package_key
                )),
            );
        };
        Ok(result)
    }

    pub fn resolve_type(
        &self,
        key: &VTableKey,
    ) -> PartialVMResult<(VTableKey, Arc<CachedDatatype>)> {
        self.get_package(&key.package_key).and_then(|pkg| {
            pkg.vtable
                .types
                .read()
                .resolve_type_by_name(&key.inner_pkg_key)
        })
    }
}

// Type-related functions over the VMDispatchTables.
impl VMDispatchTables {
    // -------------------------------------------
    // Type Depth Computations
    // -------------------------------------------
    pub fn calculate_depth_of_type(&self, datatype: &VTableKey) -> PartialVMResult<DepthFormula> {
        let mut depth_cache = BTreeMap::new();
        let depth_formula =
            self.calculate_depth_of_datatype_and_cache(datatype, &mut depth_cache)?;
        for (cache_idx, depth) in depth_cache {
            let tys = &self
                .loaded_packages
                .get(&cache_idx.package_key)
                .ok_or_else(|| {
                    PartialVMError::new(StatusCode::MISSING_DEPENDENCY).with_message(format!(
                        "Package {} not found when looking up {cache_idx:?}",
                        cache_idx.package_key
                    ))
                })?
                .vtable
                .types;
            match Arc::get_mut(&mut tys.write().type_at(&cache_idx.inner_pkg_key)) {
                Some(datatype) => {
                    // This can happen if we race for filling in the depth of the datatype which is
                    // fine as only one will win. However, if we race for filling in the depth of
                    // the then the two depths must be the same.
                    if let Some(prev_depth) = datatype.depth.as_ref() {
                        if prev_depth != &depth {
                            return Err(PartialVMError::new(
                                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            )
                            .with_message(format!(
                                "Depth calculation mismatch for {cache_idx:?}"
                            )));
                        }
                    }
                    datatype.depth = Some(depth);
                }
                None => {
                    // we have pending references to the `Arc` which is impossible,
                    // given the code that adds the `Arc` is above and no reference to
                    // it should exist.
                    // So in the spirit of not crashing we log the issue and move on leaving the
                    // datatypes depth as `None` -- if we try to access it later we will treat this
                    // as too deep.
                    error!("Arc<Datatype> cannot have any live reference while publishing");
                }
            }
        }
        Ok(depth_formula)
    }

    fn calculate_depth_of_datatype_and_cache(
        &self,
        def_idx: &VTableKey,
        depth_cache: &mut BTreeMap<VTableKey, DepthFormula>,
    ) -> PartialVMResult<DepthFormula> {
        let datatype = self.resolve_type(&def_idx.clone())?.1;
        // If we've already computed this datatypes depth, no more work remains to be done.
        if let Some(form) = &datatype.depth {
            return Ok(form.clone());
        }
        if let Some(form) = depth_cache.get(def_idx) {
            return Ok(form.clone());
        }

        let formulas = match &datatype.datatype_info {
            // The depth of enum is calculated as the maximum depth of any of its variants.
            Datatype::Enum(enum_type) => enum_type
                .variants
                .iter()
                .flat_map(|variant_type| variant_type.fields.iter())
                .map(|field_type| self.calculate_depth_of_type_and_cache(field_type, depth_cache))
                .collect::<PartialVMResult<Vec<_>>>()?,
            Datatype::Struct(struct_type) => struct_type
                .fields
                .iter()
                .map(|field_type| self.calculate_depth_of_type_and_cache(field_type, depth_cache))
                .collect::<PartialVMResult<Vec<_>>>()?,
        };
        let mut formula = DepthFormula::normalize(formulas);
        // add 1 for the struct/variant itself
        formula.add(1);
        let prev = depth_cache.insert(def_idx.clone(), formula.clone());
        if prev.is_some() {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Recursive type?".to_owned()),
            );
        }
        Ok(formula)
    }

    fn calculate_depth_of_type_and_cache(
        &self,
        ty: &Type,
        depth_cache: &mut BTreeMap<VTableKey, DepthFormula>,
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
                let mut inner = self.calculate_depth_of_type_and_cache(ty, depth_cache)?;
                // add 1 for the vector itself
                inner.add(1);
                inner
            }
            Type::TyParam(ty_idx) => DepthFormula::type_parameter(*ty_idx),
            Type::Datatype(cache_idx) => {
                let datatype_formula =
                    self.calculate_depth_of_datatype_and_cache(cache_idx, depth_cache)?;
                debug_assert!(datatype_formula.terms.is_empty());
                datatype_formula
            }
            Type::DatatypeInstantiation(inst) => {
                let (cache_idx, ty_args) = &**inst;
                let ty_arg_map = ty_args
                    .iter()
                    .enumerate()
                    .map(|(idx, ty)| {
                        let var = idx as TypeParameterIndex;
                        Ok((
                            var,
                            self.calculate_depth_of_type_and_cache(ty, depth_cache)?,
                        ))
                    })
                    .collect::<PartialVMResult<BTreeMap<_, _>>>()?;
                let datatype_formula =
                    self.calculate_depth_of_datatype_and_cache(cache_idx, depth_cache)?;

                datatype_formula.subst(ty_arg_map)?
            }
        })
    }

    pub fn type_at(&self, idx: &VTableKey) -> PartialVMResult<Arc<CachedDatatype>> {
        Ok(self.resolve_type(&idx.clone())?.1)
    }

    // -------------------------------------------
    // Helpers for loading and verification
    // -------------------------------------------

    pub(crate) fn load_type(&self, type_tag: &TypeTag) -> VMResult<Type> {
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
            TypeTag::Vector(tt) => Type::Vector(Box::new(self.load_type(tt)?)),
            TypeTag::Struct(struct_tag) => {
                let package_key = struct_tag.address;
                let string_interner = string_interner();
                let module_name = string_interner
                    .get_identifier(&struct_tag.module)
                    .map_err(|e| e.finish(Location::Undefined))?;
                let member_name = string_interner
                    .get_identifier(&struct_tag.name)
                    .map_err(|e| e.finish(Location::Undefined))?;
                let key = VTableKey {
                    package_key,
                    inner_pkg_key: IntraPackageKey {
                        module_name,
                        member_name,
                    },
                };
                let (idx, struct_type) = self
                    .resolve_type(&key)
                    .map_err(|e| e.finish(Location::Undefined))?;
                if struct_type.type_parameters.is_empty() && struct_tag.type_params.is_empty() {
                    Type::Datatype(idx)
                } else {
                    let mut type_params = vec![];
                    for ty_param in &struct_tag.type_params {
                        type_params.push(self.load_type(ty_param)?);
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
            Type::Datatype(idx) => Ok(self.type_at(idx)?.abilities),
            Type::DatatypeInstantiation(inst) => {
                let (idx, type_args) = &**inst;
                let datatype_type = self.type_at(idx)?;
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
    // Type Translation Helpers
    // -------------------------------------------

    fn read_cached_struct_tag(
        &self,
        gidx: &VTableKey,
        ty_args: &[Type],
        tag_type: DatatypeTagType,
    ) -> Option<StructTag> {
        let pkg = self.get_package(&gidx.package_key).ok()?;
        let cache = pkg.vtable.types.read();
        let info = cache
            .cached_instantiations
            .get(&gidx.inner_pkg_key)?
            .get(ty_args)?;

        match tag_type {
            DatatypeTagType::Runtime => info.runtime_tag.clone(),
            DatatypeTagType::Defining => info.defining_tag.clone(),
        }
    }

    fn datatype_gidx_to_type_tag(
        &self,
        gidx: &VTableKey,
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
        let datatype = self.type_at(gidx)?;

        let pkg = self.get_package(&gidx.package_key)?;
        let mut cache = pkg.vtable.types.write();
        let info = cache
            .cached_instantiations
            .entry(gidx.inner_pkg_key.clone())
            .or_default()
            .entry(ty_args.to_vec())
            .or_default();

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
        &self,
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
                gidx,
                &[],
                tag_type,
            )?)),
            Type::DatatypeInstantiation(struct_inst) => {
                let (gidx, ty_args) = &**struct_inst;
                TypeTag::Struct(Box::new(
                    self.datatype_gidx_to_type_tag(gidx, ty_args, tag_type)?,
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
        &self,
        gidx: &VTableKey,
        ty_args: &[Type],
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<runtime_value::MoveDatatypeLayout> {
        if let Some(type_info) = self
            .get_package(&gidx.package_key)?
            .vtable
            .types
            .read()
            .cached_instantiations
            .get(&gidx.inner_pkg_key)
            .and_then(|type_map| type_map.get(ty_args))
        {
            if let Some(node_count) = &type_info.node_count {
                *count += *node_count
            }
            if let Some(layout) = &type_info.layout {
                return Ok(layout.clone());
            }
        }

        let count_before = *count;
        let ty = self.type_at(gidx)?;
        let type_layout = match ty.datatype_info {
            Datatype::Enum(ref einfo) => {
                let mut variant_layouts = vec![];
                for variant in einfo.variants.iter() {
                    let field_tys = variant
                        .fields
                        .iter()
                        .map(|ty| type_cache::subst(ty, ty_args))
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
                    .map(|ty| type_cache::subst(ty, ty_args))
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

        let pkg = self.get_package(&gidx.package_key)?;
        let mut cache = pkg.vtable.types.write();
        let info = cache
            .cached_instantiations
            .entry(gidx.inner_pkg_key.clone())
            .or_default()
            .entry(ty_args.to_vec())
            .or_default();
        info.layout = Some(type_layout.clone());
        info.node_count = Some(field_node_count);

        Ok(type_layout)
    }

    fn type_to_type_layout_impl(
        &self,
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
                .type_gidx_to_type_layout(gidx, &[], count, depth)?
                .into_layout(),
            Type::DatatypeInstantiation(inst) => {
                let (gidx, ty_args) = &**inst;
                self.type_gidx_to_type_layout(gidx, ty_args, count, depth)?
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
        &self,
        gidx: &VTableKey,
        ty_args: &[Type],
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<annotated_value::MoveDatatypeLayout> {
        if let Some(datatype_info) = self
            .get_package(&gidx.package_key)?
            .vtable
            .types
            .read()
            .cached_instantiations
            .get(&gidx.inner_pkg_key)
            .and_then(|type_map| type_map.get(ty_args))
        {
            if let Some(annotated_node_count) = &datatype_info.annotated_node_count {
                *count += *annotated_node_count
            }
            if let Some(layout) = &datatype_info.annotated_layout {
                return Ok(layout.clone());
            }
        }

        let count_before = *count;
        let ty = self.type_at(gidx)?;
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
                            let ty = type_cache::subst(ty, ty_args)?;
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
                        let ty = type_cache::subst(ty, ty_args)?;
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

        let pkg = self.get_package(&gidx.package_key)?;
        let mut cache = pkg.vtable.types.write();
        let info = cache
            .cached_instantiations
            .entry(gidx.inner_pkg_key.clone())
            .or_default()
            .entry(ty_args.to_vec())
            .or_default();
        info.annotated_layout = Some(type_layout.clone());
        info.annotated_node_count = Some(field_node_count);

        Ok(type_layout)
    }

    fn type_to_fully_annotated_layout_impl(
        &self,
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
                .datatype_gidx_to_fully_annotated_layout(gidx, &[], count, depth)?
                .into_layout(),
            Type::DatatypeInstantiation(inst) => {
                let (gidx, ty_args) = &**inst;
                self.datatype_gidx_to_fully_annotated_layout(gidx, ty_args, count, depth)?
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

    pub(crate) fn type_to_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_to_type_tag_impl(ty, DatatypeTagType::Defining)
    }

    pub(crate) fn type_to_runtime_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_to_type_tag_impl(ty, DatatypeTagType::Runtime)
    }

    pub(crate) fn type_to_type_layout(
        &self,
        ty: &Type,
    ) -> PartialVMResult<runtime_value::MoveTypeLayout> {
        let mut count = 0;
        self.type_to_type_layout_impl(ty, &mut count, 1)
    }

    pub(crate) fn type_to_fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> PartialVMResult<annotated_value::MoveTypeLayout> {
        let mut count = 0;
        self.type_to_fully_annotated_layout_impl(ty, &mut count, 1)
    }

    // -------------------------------------------
    // Public APIs for type layout retrieval.
    // -------------------------------------------

    #[allow(dead_code)]
    pub(crate) fn get_type_layout(
        &self,
        type_tag: &TypeTag,
    ) -> VMResult<runtime_value::MoveTypeLayout> {
        let ty = self.load_type(type_tag)?;
        self.type_to_type_layout(&ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    #[allow(dead_code)]
    pub(crate) fn get_fully_annotated_type_layout(
        &self,
        type_tag: &TypeTag,
    ) -> VMResult<annotated_value::MoveTypeLayout> {
        let ty = self.load_type(type_tag)?;
        self.type_to_fully_annotated_layout(&ty)
            .map_err(|e| e.finish(Location::Undefined))
    }
}
