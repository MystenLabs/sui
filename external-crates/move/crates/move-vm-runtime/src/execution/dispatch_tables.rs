// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This module is responsible for the building of the package VTables given a root package storage
// ID. The VTables are built by loading all the packages that are dependencies of the root package,
// and once they are loaded creating the VTables for each package, and populating the
// `loaded_packages` table (keyed by the _runtime_ package ID!) with the VTables for each package
// in the transitive closure of the root package.

use crate::{
    cache::identifier_interner::{intern_identifier, resolve_interned, IdentifierKey},
    jit::execution::ast::{
        ArenaType, Datatype, DatatypeDescriptor, Function, Module, Package, Type, TypeNodeCount,
        TypeSubst,
    },
    shared::{
        constants::{
            HISTORICAL_MAX_TYPE_TO_LAYOUT_NODES, MAX_TYPE_INSTANTIATION_NODES, VALUE_DEPTH_MAX,
        },
        types::{DefiningTypeId, OriginalId},
        vm_pointer::VMPointer,
    },
};
use move_binary_format::{
    errors::{Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{AbilitySet, TypeParameterIndex},
};
use move_core_types::{
    annotated_value,
    identifier::Identifier,
    language_storage::{ModuleId, StructTag, TypeTag},
    runtime_value,
    vm_status::StatusCode,
};

use move_vm_config::runtime::VMConfig;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

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
#[derive(Debug)]
pub struct VMDispatchTables {
    pub(crate) vm_config: Arc<VMConfig>,
    pub(crate) loaded_packages: BTreeMap<OriginalId, Arc<Package>>,
    /// Representation of runtime type depths. This is separate from the underlying packages to
    /// avoid grabbing write-locks and toward the possibility these may change based on linkage
    /// (e.g., type ugrades or similar).
    /// [SAFETY] Ordering of inner maps is not guaranteed
    pub(crate) type_depths: BTreeMap<OriginalId, DefinitionMap<DepthFormula>>,
    /// Defining ID Set -- a set of all defining IDs on any types mentioned in the package.
    /// [SAFETY] Ordering is not guaranteed
    #[allow(dead_code)]
    pub(crate) defining_id_origins: BTreeMap<DefiningTypeId, OriginalId>,
}

/// A `PackageVTable` is a collection of pointers indexed by the module and name
/// within the package.
#[derive(Debug)]
pub struct PackageVirtualTable {
    /// Representation of runtime functions.
    pub functions: DefinitionMap<VMPointer<Function>>,
    /// Representation of runtime types.
    pub types: DefinitionMap<VMPointer<DatatypeDescriptor>>,
    /// Defining ID Set -- a set of all defining IDs on any types mentioned in the package.
    pub defining_ids: BTreeSet<DefiningTypeId>,
}

/// This is a lookup-only map for recording information about module members in loaded package
/// modules. It exposes an intentionally spartan interface to prevent any unexpected behavior
/// (e.g., unstable iteration ordering) that Rust's standard collections run afoul of.
#[derive(Debug)]
pub struct DefinitionMap<Value>(HashMap<IntraPackageKey, Value>);

/// runtime_address::module_name::function_name
/// NB: This relies on no boxing -- if this introduces boxes, the arena allocation in the execution
/// AST will leak memory.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct VirtualTableKey {
    pub package_key: OriginalId,
    pub inner_pkg_key: IntraPackageKey,
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct IntraPackageKey {
    pub module_name: IdentifierKey,
    pub member_name: IdentifierKey,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum DatatypeTagType {
    Runtime,
    Defining,
}

/// A formula for the maximum depth of the value for a type
/// max(Ti + Ci, ..., CBase)
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug)]
pub struct DepthFormula {
    /// The terms for each type parameter, if present.
    /// Ti + Ci
    pub terms: Vec<(TypeParameterIndex, u64)>,
    /// The depth for any non type parameter term, if one exists.
    /// CBase
    pub constant: Option<u64>,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

// ------------------------------------------------------------------------
// The VM API that it will use to resolve packages and functions during execution of the
// transaction.
// ------------------------------------------------------------------------

impl VMDispatchTables {
    /// Create a new RuntimeVTables instance.
    /// NOTE: This assumes linkage has already occured.
    pub(crate) fn new(
        vm_config: Arc<VMConfig>,
        loaded_packages: BTreeMap<OriginalId, Arc<Package>>,
    ) -> VMResult<Self> {
        let defining_id_origins = {
            let mut defining_id_map = BTreeMap::new();
            for (addr, pkg) in &loaded_packages {
                for defining_id in &pkg.vtable.defining_ids {
                    if let Some(prev) = defining_id_map.insert(*defining_id, *addr) {
                        return Err(PartialVMError::new(
                            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        )
                        .with_message(format!(
                            "Defining ID {defining_id} found for {addr} and {prev}"
                        ))
                        .finish(Location::Undefined));
                    }
                }
            }
            defining_id_map
        };
        Ok(Self {
            vm_config,
            loaded_packages,
            defining_id_origins,
            type_depths: BTreeMap::new(),
        })
    }

    pub fn get_package(&self, id: &OriginalId) -> PartialVMResult<Arc<Package>> {
        self.loaded_packages.get(id).cloned().ok_or_else(|| {
            PartialVMError::new(StatusCode::VTABLE_KEY_LOOKUP_ERROR)
                .with_message(format!("Package {} not found", id))
        })
    }

    pub fn resolve_loaded_module(
        &self,
        runtime_id: &ModuleId,
    ) -> PartialVMResult<VMPointer<Module>> {
        let (package, module_id) = runtime_id.into();
        let package = self.loaded_packages.get(package).ok_or_else(|| {
            PartialVMError::new(StatusCode::VTABLE_KEY_LOOKUP_ERROR)
                .with_message(format!("Package {} not found", package))
        })?;
        package
            .loaded_modules
            .get(module_id)
            .map(VMPointer::from_ref)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::VTABLE_KEY_LOOKUP_ERROR)
                    .with_message(format!("Module {} not found", module_id))
            })
    }

    pub fn resolve_function(
        &self,
        vtable_key: &VirtualTableKey,
    ) -> PartialVMResult<VMPointer<Function>> {
        let Some(pkg) = self.loaded_packages.get(&vtable_key.package_key) else {
            return Err(PartialVMError::new(StatusCode::VTABLE_KEY_LOOKUP_ERROR)
                .with_message(format!("Could not find package {}", vtable_key.package_key)));
        };
        if let Some(function_) = pkg.vtable.functions.get(&vtable_key.inner_pkg_key) {
            Ok(function_.ptr_clone())
        } else {
            Err(
                PartialVMError::new(StatusCode::VTABLE_KEY_LOOKUP_ERROR).with_message(format!(
                    "Could not find function {}",
                    vtable_key.to_string()?
                )),
            )
        }
    }

    pub fn resolve_type(
        &self,
        vtable_key: &VirtualTableKey,
    ) -> PartialVMResult<VMPointer<DatatypeDescriptor>> {
        let Some(pkg) = self.loaded_packages.get(&vtable_key.package_key) else {
            return Err(PartialVMError::new(StatusCode::VTABLE_KEY_LOOKUP_ERROR)
                .with_message(format!("Could not find package {}", vtable_key.package_key)));
        };
        if let Some(type_) = pkg.vtable.types.get(&vtable_key.inner_pkg_key) {
            Ok(type_.ptr_clone())
        } else {
            Err(PartialVMError::new(StatusCode::VTABLE_KEY_LOOKUP_ERROR)
                .with_message(format!("Could not find type {}", vtable_key.to_string()?)))
        }
    }
}

// ------------------------------------------------------------------------
// Type-related functions over the VMDispatchTables.
// ------------------------------------------------------------------------

impl VMDispatchTables {
    // -------------------------------------------
    // Helpers for loading and verification
    // -------------------------------------------

    // Load a type from a TypeTag into a VM type.
    // NB: the type `TypeTag` _must_ be defining ID based. Otherwise, the type resolution will
    // fail.
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
            // NB: Note that this tag is slightly misnamed and used for all Datatypes.
            TypeTag::Struct(struct_tag) => {
                let defining_id = struct_tag.address;
                let package_key = *self.defining_id_origins.get(&defining_id).ok_or_else(|| {
                    PartialVMError::new(StatusCode::TYPE_RESOLUTION_FAILURE)
                        .with_message(format!(
                            "Defining ID {defining_id} for type {type_tag} not found in loaded packages"
                        ))
                        .finish(Location::Undefined)
                })?;
                let module_name = intern_identifier(&struct_tag.module)
                    .map_err(|e| e.finish(Location::Undefined))?;
                let member_name = intern_identifier(&struct_tag.name)
                    .map_err(|e| e.finish(Location::Undefined))?;
                let key = VirtualTableKey {
                    package_key,
                    inner_pkg_key: IntraPackageKey {
                        module_name,
                        member_name,
                    },
                };
                let datatype = self
                    .resolve_type(&key)
                    .map_err(|e| e.finish(Location::Undefined))?
                    .to_ref();

                // The computed runtime ID should match the runtime ID of the datatype that we have
                // loaded.
                if datatype.runtime_id.address() != &package_key {
                    return Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message(format!(
                                "Runtime ID resolution of {defining_id} => {package_key} does not match runtime ID of loaded type: {}",
                                datatype.runtime_id.address()

                            ))
                            .finish(Location::Undefined),
                    );
                }
                // The defining ID should match the defining ID of the datatype that we
                // have loaded.
                if datatype.defining_id.address() != &defining_id {
                    return Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message(format!(
                                "Defining ID {defining_id} does not match defining ID {defining_id}"
                            ))
                            .finish(Location::Undefined),
                    );
                }
                if datatype.type_parameters().is_empty() && struct_tag.type_params.is_empty() {
                    Type::Datatype(key)
                } else {
                    let mut type_params = vec![];
                    for ty_param in &struct_tag.type_params {
                        type_params.push(self.load_type(ty_param)?);
                    }
                    self.verify_ty_args(datatype.type_param_constraints(), &type_params)
                        .map_err(|e| e.finish(Location::Undefined))?;
                    Type::DatatypeInstantiation(Box::new((key, type_params)))
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
            Type::Datatype(idx) => Ok(*self.resolve_type(idx)?.to_ref().abilities()),
            Type::DatatypeInstantiation(inst) => {
                let (idx, type_args) = &**inst;
                let datatype_type = self.resolve_type(idx)?.to_ref();
                let declared_phantom_parameters = datatype_type
                    .type_parameters()
                    .iter()
                    .map(|param| param.is_phantom);
                let type_argument_abilities = type_args
                    .iter()
                    .map(|arg| self.abilities(arg))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                AbilitySet::polymorphic_abilities(
                    *datatype_type.abilities(),
                    declared_phantom_parameters,
                    type_argument_abilities,
                )
            }
        }
    }

    // -------------------------------------------
    // Type Depth Computations
    // -------------------------------------------
    // These functions compute the depth of the specified type, plus cache the depth formula of any
    // uncached subtypes they find along the way.

    fn cached_type_depth(&self, datatype: &VirtualTableKey) -> Option<&DepthFormula> {
        let package = self.type_depths.get(&datatype.package_key)?;
        package.get(&datatype.inner_pkg_key)
    }

    pub fn calculate_depth_of_type(
        &mut self,
        datatype_name: &VirtualTableKey,
    ) -> PartialVMResult<DepthFormula> {
        let mut depth_cache = HashMap::new();
        let depth_formula =
            self.calculate_depth_of_datatype_and_cache(datatype_name, &mut depth_cache)?;
        for (datatype_key, depth) in depth_cache {
            self.type_depths
                .entry(datatype_key.package_key)
                .or_default()
                .insert(datatype_key.inner_pkg_key, depth)?;
        }
        Ok(depth_formula)
    }

    fn calculate_depth_of_datatype_and_cache(
        &self,
        datatype_name: &VirtualTableKey,
        depth_cache: &mut HashMap<VirtualTableKey, DepthFormula>,
    ) -> PartialVMResult<DepthFormula> {
        // If we've already computed this datatypes depth, no more work remains to be done.
        if let Some(form) = self.cached_type_depth(datatype_name) {
            return Ok(form.clone());
        }
        if let Some(form) = depth_cache.get(datatype_name) {
            return Ok(form.clone());
        }

        let datatype = self.resolve_type(&datatype_name.clone())?.to_ref();
        let formulas = match datatype.datatype_info.inner_ref() {
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
        let prev = depth_cache.insert(datatype_name.clone(), formula.clone());
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
        ty: &ArenaType,
        depth_cache: &mut HashMap<VirtualTableKey, DepthFormula>,
    ) -> PartialVMResult<DepthFormula> {
        Ok(match ty {
            ArenaType::Bool
            | ArenaType::U8
            | ArenaType::U64
            | ArenaType::U128
            | ArenaType::Address
            | ArenaType::Signer
            | ArenaType::U16
            | ArenaType::U32
            | ArenaType::U256 => DepthFormula::constant(1),
            // we should not see the reference here, we could instead give an invariant violation
            ArenaType::Vector(ty) | ArenaType::Reference(ty) | ArenaType::MutableReference(ty) => {
                let mut inner = self.calculate_depth_of_type_and_cache(ty, depth_cache)?;
                // add 1 for the vector itself
                inner.add(1);
                inner
            }
            ArenaType::TyParam(ty_idx) => DepthFormula::type_parameter(*ty_idx),
            ArenaType::Datatype(cache_idx) => {
                let datatype_formula =
                    self.calculate_depth_of_datatype_and_cache(cache_idx, depth_cache)?;
                debug_assert!(datatype_formula.terms.is_empty());
                datatype_formula
            }
            ArenaType::DatatypeInstantiation(inst) => {
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

    // -------------------------------------------
    // Type Translation Helpers
    // -------------------------------------------

    fn datatype_to_type_tag(
        &self,
        datatype_name: &VirtualTableKey,
        ty_args: &[Type],
        tag_type: DatatypeTagType,
    ) -> PartialVMResult<StructTag> {
        let type_params = ty_args
            .iter()
            .map(|ty| self.type_to_type_tag_impl(ty, tag_type))
            .collect::<PartialVMResult<Vec<_>>>()?;
        let datatype = self.resolve_type(datatype_name)?.to_ref();

        let (address, module) = match tag_type {
            DatatypeTagType::Runtime => (
                *datatype.runtime_id.address(),
                datatype.runtime_id.name().to_owned(),
            ),

            DatatypeTagType::Defining => (
                *datatype.defining_id.address(),
                datatype.defining_id.name().to_owned(),
            ),
        };
        let name = resolve_interned(&datatype.name, "datatype name")?;

        let tag = StructTag {
            address,
            module,
            name,
            type_params,
        };
        Ok(tag)
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
            Type::Datatype(gidx) => {
                TypeTag::Struct(Box::new(self.datatype_to_type_tag(gidx, &[], tag_type)?))
            }
            Type::DatatypeInstantiation(struct_inst) => {
                let (gidx, ty_args) = &**struct_inst;
                TypeTag::Struct(Box::new(
                    self.datatype_to_type_tag(gidx, ty_args, tag_type)?,
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

    fn datatype_to_type_layout(
        &self,
        datatype_name: &VirtualTableKey,
        ty_args: &[Type],
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<runtime_value::MoveDatatypeLayout> {
        let ty = self.resolve_type(datatype_name)?.to_ref();
        // TODO(vm-rewrite): update this code
        let type_layout = match ty.datatype_info.inner_ref() {
            Datatype::Enum(ref einfo) => {
                let mut variant_layouts = vec![];
                *count += einfo.variants.len() as u64;
                for variant in einfo.variants.iter() {
                    let field_tys = variant
                        .fields
                        .iter()
                        .map(|ty| checked_subst(ty, ty_args))
                        .collect::<PartialVMResult<Vec<_>>>()?;
                    let field_layouts = field_tys
                        .iter()
                        .map(|ty| self.type_to_type_layout_impl(ty, count, depth + 1))
                        .collect::<PartialVMResult<Vec<_>>>()?;
                    variant_layouts.push(field_layouts);
                }
                runtime_value::MoveDatatypeLayout::Enum(Box::new(runtime_value::MoveEnumLayout(
                    Box::new(variant_layouts),
                )))
            }
            Datatype::Struct(ref sinfo) => {
                let field_tys = sinfo
                    .fields
                    .iter()
                    .map(|ty| checked_subst(ty, ty_args))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let field_layouts = field_tys
                    .iter()
                    .map(|ty| self.type_to_type_layout_impl(ty, count, depth + 1))
                    .collect::<PartialVMResult<Vec<_>>>()?;

                runtime_value::MoveDatatypeLayout::Struct(Box::new(
                    runtime_value::MoveStructLayout::new(field_layouts),
                ))
            }
        };
        Ok(type_layout)
    }

    fn type_to_type_layout_impl(
        &self,
        ty: &Type,
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<runtime_value::MoveTypeLayout> {
        self.check_count_and_depth(count, &depth)?;
        *count += 1;
        let result = match ty {
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
                .datatype_to_type_layout(gidx, &[], count, depth)?
                .into_layout(),
            Type::DatatypeInstantiation(inst) => {
                let (gidx, ty_args) = &**inst;
                self.datatype_to_type_layout(gidx, ty_args, count, depth)?
                    .into_layout()
            }
            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("no type layout for {:?}", ty)),
                );
            }
        };
        self.check_count_and_depth(count, &depth)?;
        Ok(result)
    }

    fn datatype_to_fully_annotated_layout(
        &self,
        datatype_name: &VirtualTableKey,
        ty_args: &[Type],
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<annotated_value::MoveDatatypeLayout> {
        let ty = self.resolve_type(datatype_name)?.to_ref();
        let struct_tag =
            self.datatype_to_type_tag(datatype_name, ty_args, DatatypeTagType::Defining)?;
        let type_layout = match ty.datatype_info.inner_ref() {
            Datatype::Enum(enum_type) => {
                let mut variant_layouts = BTreeMap::new();
                *count += enum_type.variants.len() as u64;
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
                            let n = resolve_interned(n, "field name")?;
                            let ty = checked_subst(ty, ty_args)?;
                            let l =
                                self.type_to_fully_annotated_layout_impl(&ty, count, depth + 1)?;
                            Ok(annotated_value::MoveFieldLayout::new(n, l))
                        })
                        .collect::<PartialVMResult<Vec<_>>>()?;
                    variant_layouts.insert(
                        (
                            resolve_interned(&variant.variant_name, "variant name")?,
                            variant.variant_tag,
                        ),
                        field_layouts,
                    );
                }
                annotated_value::MoveDatatypeLayout::Enum(Box::new(
                    annotated_value::MoveEnumLayout {
                        type_: struct_tag.clone(),
                        variants: variant_layouts,
                    },
                ))
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
                    .zip(struct_type.fields.iter())
                    .map(|(n, ty)| {
                        let n = resolve_interned(n, "field name")?;
                        let ty = checked_subst(ty, ty_args)?;
                        let l = self.type_to_fully_annotated_layout_impl(&ty, count, depth + 1)?;
                        Ok(annotated_value::MoveFieldLayout::new(n, l))
                    })
                    .collect::<PartialVMResult<Vec<_>>>()?;
                annotated_value::MoveDatatypeLayout::Struct(Box::new(
                    annotated_value::MoveStructLayout::new(struct_tag, field_layouts),
                ))
            }
        };
        Ok(type_layout)
    }

    fn type_to_fully_annotated_layout_impl(
        &self,
        ty: &Type,
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<annotated_value::MoveTypeLayout> {
        self.check_count_and_depth(count, &depth)?;
        *count += 1;
        let result = match ty {
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
                .datatype_to_fully_annotated_layout(gidx, &[], count, depth)?
                .into_layout(),
            Type::DatatypeInstantiation(inst) => {
                let (gidx, ty_args) = &**inst;
                self.datatype_to_fully_annotated_layout(gidx, ty_args, count, depth)?
                    .into_layout()
            }
            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("no type layout for {:?}", ty)),
                );
            }
        };
        self.check_count_and_depth(count, &depth)?;
        Ok(result)
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

    pub(crate) fn arena_type_to_fully_annotated_layout(
        &self,
        ty: &ArenaType,
    ) -> PartialVMResult<annotated_value::MoveTypeLayout> {
        self.type_to_fully_annotated_layout(&ty.to_type())
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

    pub(crate) fn get_type_layout(
        &self,
        type_tag: &TypeTag,
    ) -> VMResult<runtime_value::MoveTypeLayout> {
        let ty = self.load_type(type_tag)?;
        self.type_to_type_layout(&ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    pub(crate) fn get_fully_annotated_type_layout(
        &self,
        type_tag: &TypeTag,
    ) -> VMResult<annotated_value::MoveTypeLayout> {
        let ty = self.load_type(type_tag)?;
        self.type_to_fully_annotated_layout(&ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    fn check_count_and_depth(&self, count: &u64, depth: &u64) -> PartialVMResult<()> {
        if *count
            > self
                .vm_config
                .max_type_to_layout_nodes
                .unwrap_or(HISTORICAL_MAX_TYPE_TO_LAYOUT_NODES)
        {
            return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
        }
        if *depth > VALUE_DEPTH_MAX {
            return Err(PartialVMError::new(StatusCode::VM_MAX_VALUE_DEPTH_REACHED));
        }
        Ok(())
    }
}

impl DepthFormula {
    /// A value with no type parameters
    pub fn constant(constant: u64) -> Self {
        Self {
            terms: vec![],
            constant: Some(constant),
        }
    }

    /// A stand alone type parameter value
    pub fn type_parameter(tparam: TypeParameterIndex) -> Self {
        Self {
            terms: vec![(tparam, 0)],
            constant: None,
        }
    }

    /// We `max` over a list of formulas, and we normalize it to deal with duplicate terms, e.g.
    /// `max(max(t1 + 1, t2 + 2, 2), max(t1 + 3, t2 + 1, 4))` becomes
    /// `max(t1 + 3, t2 + 2, 4)`
    pub fn normalize(formulas: Vec<Self>) -> Self {
        let mut var_map = BTreeMap::new();
        let mut constant_acc = None;
        for formula in formulas {
            let Self { terms, constant } = formula;
            for (var, cur_factor) in terms {
                var_map
                    .entry(var)
                    .and_modify(|prev_factor| {
                        *prev_factor = std::cmp::max(cur_factor, *prev_factor)
                    })
                    .or_insert(cur_factor);
            }
            match (constant_acc, constant) {
                (_, None) => (),
                (None, Some(_)) => constant_acc = constant,
                (Some(c1), Some(c2)) => constant_acc = Some(std::cmp::max(c1, c2)),
            }
        }
        Self {
            terms: var_map.into_iter().collect(),
            constant: constant_acc,
        }
    }

    /// Substitute in formulas for each type parameter and normalize the final formula
    pub fn subst(
        &self,
        mut map: BTreeMap<TypeParameterIndex, DepthFormula>,
    ) -> PartialVMResult<DepthFormula> {
        let Self { terms, constant } = self;
        let mut formulas = vec![];
        if let Some(constant) = constant {
            formulas.push(DepthFormula::constant(*constant))
        }
        for (t_i, c_i) in terms {
            let Some(mut u_form) = map.remove(t_i) else {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("{t_i:?} missing mapping")),
                );
            };
            u_form.add(*c_i);
            formulas.push(u_form)
        }
        Ok(DepthFormula::normalize(formulas))
    }

    /// Given depths for each type parameter, solve the formula giving the max depth for the type
    pub fn solve(&self, tparam_depths: &[u64]) -> PartialVMResult<u64> {
        let Self { terms, constant } = self;
        let mut depth = constant.as_ref().copied().unwrap_or(0);
        for (t_i, c_i) in terms {
            match tparam_depths.get(*t_i as usize) {
                None => {
                    return Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message(format!("{t_i:?} missing mapping")),
                    )
                }
                Some(ty_depth) => depth = std::cmp::max(depth, ty_depth.saturating_add(*c_i)),
            }
        }
        Ok(depth)
    }

    // `max(t_0 + c_0, ..., t_n + c_n, c_base) + c`. But our representation forces us to distribute
    // the addition, so it becomes `max(t_0 + c_0 + c, ..., t_n + c_n + c, c_base + c)`
    pub fn add(&mut self, c: u64) {
        let Self { terms, constant } = self;
        for (_t_i, c_i) in terms {
            *c_i = (*c_i).saturating_add(c);
        }
        if let Some(cbase) = constant.as_mut() {
            *cbase = (*cbase).saturating_add(c);
        }
    }
}

// ------------------------------------------------------------------------
// Other Impls
// ------------------------------------------------------------------------

impl PackageVirtualTable {
    pub fn new(
        functions: DefinitionMap<VMPointer<Function>>,
        types: DefinitionMap<VMPointer<DatatypeDescriptor>>,
    ) -> Self {
        // [SAFETY] This is unordered, but okay because we are making a set anyway.
        let defining_ids = types
            .0
            .values()
            .map(|ty| ty.defining_id.address())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .copied()
            .collect();
        Self {
            functions,
            types,
            defining_ids,
        }
    }
}

impl<T> DefinitionMap<T> {
    /// Create a new, empty DefinitionMap
    pub fn empty() -> Self {
        Self(HashMap::new())
    }

    /// Indicates if the DefinitionMap is empty or not.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of entries in the map
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Extends a DefintionMap with new entries, producing an error if a duplicate key is found.
    pub fn extend(
        &mut self,
        items: impl IntoIterator<Item = (IntraPackageKey, T)>,
    ) -> PartialVMResult<()> {
        let map = &mut self.0;
        for (name, value) in items {
            if map.insert(name, value).is_some() {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("Duplicate key {}", name.to_string()?)),
                );
            }
        }
        Ok(())
    }

    /// Creates a new DefintionMap, producing an error if a duplicate key is found.
    pub fn from_unique(
        items: impl IntoIterator<Item = (IntraPackageKey, T)>,
    ) -> PartialVMResult<Self> {
        let mut map = HashMap::new();
        for (name, value) in items {
            if map.insert(name, value).is_some() {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("Duplicate key {}", name.to_string()?)),
                );
            }
        }
        Ok(Self(map))
    }

    /// Retrieve a key from the definition map.
    pub fn get(&self, key: &IntraPackageKey) -> Option<&T> {
        self.0.get(key)
    }

    /// Private to this module, as it is only used in depth formula calculations.
    fn insert(&mut self, key: IntraPackageKey, value: T) -> PartialVMResult<()> {
        if self.0.insert(key, value).is_some() {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("Duplicate key {}", key.to_string()?)),
            );
        }
        Ok(())
    }
}

impl VirtualTableKey {
    pub fn new(package_key: OriginalId, inner_pkg_key: IntraPackageKey) -> Self {
        Self {
            package_key,
            inner_pkg_key,
        }
    }

    pub fn from_parts(
        package_key: OriginalId,
        module_name: IdentifierKey,
        member_name: IdentifierKey,
    ) -> Self {
        Self {
            package_key,
            inner_pkg_key: IntraPackageKey {
                module_name,
                member_name,
            },
        }
    }

    pub fn module_id(&self) -> PartialVMResult<ModuleId> {
        let module_name = resolve_interned(&self.inner_pkg_key.module_name, "module name")?;
        Ok(ModuleId::new(self.package_key, module_name))
    }

    pub fn member_name(&self) -> PartialVMResult<Identifier> {
        resolve_interned(&self.inner_pkg_key.member_name, "member name")
    }

    pub fn to_string(&self) -> PartialVMResult<String> {
        let inner_name = self.inner_pkg_key.to_string()?;
        Ok(format!("{}::{}", self.package_key, inner_name))
    }

    pub fn to_short_string(&self) -> PartialVMResult<String> {
        let inner_name = self.inner_pkg_key.to_string()?;
        Ok(format!(
            "0x{}::{}",
            self.package_key.short_str_lossless(),
            inner_name,
        ))
    }
}

impl IntraPackageKey {
    pub fn to_string(&self) -> PartialVMResult<String> {
        let module_name = resolve_interned(&self.module_name, "module name")?;
        let member_name = resolve_interned(&self.member_name, "member name")?;
        Ok(format!("{}::{}", module_name, member_name))
    }
}

// -------------------------------------------------------------------------------------------------
// Default
// -------------------------------------------------------------------------------------------------

impl<T> Default for DefinitionMap<T> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

// -------------------------------------------------------------------------------------------------
// Helper Functions
// -------------------------------------------------------------------------------------------------

// Return an instantiated type given a generic and an instantiation.
// Stopgap to avoid a recursion that is either taking too long or using too
// much memory
pub(crate) fn checked_subst(ty: &ArenaType, ty_args: &[Type]) -> PartialVMResult<Type> {
    // Before instantiating the type, count the # of nodes of all type arguments plus
    // existing type instantiation.
    // If that number is larger than MAX_TYPE_INSTANTIATION_NODES, refuse to construct this type.
    // This prevents constructing larger and larger types via datatype instantiation.
    if let ArenaType::DatatypeInstantiation(inst) = ty {
        let (_, type_params) = &**inst;
        let mut sum_nodes = 1u64;
        for ty in type_params.iter() {
            sum_nodes = sum_nodes.saturating_add(ty.count_type_nodes());
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
            }
        }
        for ty in ty_args.iter() {
            sum_nodes = sum_nodes.saturating_add(ty.count_type_nodes());
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
            }
        }
    }
    ty.subst(ty_args)
}
