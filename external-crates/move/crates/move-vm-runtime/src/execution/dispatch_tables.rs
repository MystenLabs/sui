// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This module is responsible for the building of the package VTables given a root package storage
// ID. The VTables are built by loading all the packages that are dependencies of the root package,
// and once they are loaded creating the VTables for each package, and populating the
// `loaded_packages` table (keyed by the _runtime_ package ID!) with the VTables for each package
// in the transitive closure of the root package.

use crate::{
    cache::identifier_interner::{IdentifierInterner, IdentifierKey},
    execution::vm::DatatypeInfo,
    jit::execution::ast::{
        ArenaType, Datatype, DatatypeDescriptor, DatatypeMeasure, FieldVar, FormulatedType,
        Function, FunctionInstantiation, Package, StructInstantiation, Type, TypeArguments,
        TypeMeasure, TypeSizes, TypeSubst, VariantInstantiation,
    },
    shared::{
        TypeSize,
        constants::{
            HISTORICAL_MAX_TYPE_TO_LAYOUT_NODES, MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_LRU_SIZE,
            VALUE_DEPTH_MAX,
        },
        linkage_context::LinkageContext,
        types::{DefiningTypeId, OriginalId},
        vm_pointer::VMPointer,
    },
};

use move_binary_format::{
    checked_as,
    errors::{Location, PartialVMResult, VMResult},
    file_format::{AbilitySet, TypeParameterIndex},
    partial_vm_error,
};
use move_core_types::{
    annotated_value,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
    runtime_value,
};
use move_vm_config::runtime::VMConfig;

use quick_cache::unsync::Cache as QCache;
use tracing::instrument;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, Mutex},
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
/// This structure may be cached and reused across transactions with identical linkages, so we add
/// an Arc<> around the inner data structures to facilitate more-efficient sharing.
///
/// FUTURE(vm-rewrite): The representation can be optimized to use a more efficient data structure
/// for vtable/cross-package function resolution but we will keep it simple for now.
#[derive(Debug)]
pub struct VMDispatchTables {
    pub(crate) vm_config: Arc<VMConfig>,
    pub(crate) interner: Arc<IdentifierInterner>,
    pub(crate) loaded_packages: Arc<BTreeMap<OriginalId, Arc<Package>>>,
    /// Defining ID Set -- a set of all defining IDs on any types defined in the package.
    /// [SAFETY] Ordering is not guaranteed
    pub(crate) defining_id_origins: Arc<BTreeMap<DefiningTypeId, OriginalId>>,
    pub(crate) link_context: Arc<LinkageContext>,
    /// Representation of runtime through-field type measures (value depth and layout size).
    /// This is separate from the underlying packages to avoid grabbing write-locks and toward
    /// the possibility these may change based on linkage (e.g., type ugrades or similar).
    /// [SAFETY] Ordering of inner maps is not guaranteed
    /// NB: This cache is mutated during execution (behind the `Mutex`, so shared borrowers of
    /// the dispatch tables — e.g. natives requesting layouts — can hit it), so we make a new
    /// one for each VM instantiation.
    ///
    /// However, the contents of the cache do not affect execution correctness, only performance.
    pub(crate) type_measures: Mutex<QCache<VirtualTableKey, ResolvedMeasure>>,
}

/// A `PackageVTable` is a collection of pointers indexed by the module and name
/// within the package.
#[derive(Debug)]
pub struct PackageVirtualTable {
    /// Representation of runtime functions.
    pub(crate) functions: DefinitionMap<VMPointer<Function>>,
    /// Representation of runtime types.
    pub(crate) types: DefinitionMap<VMPointer<DatatypeDescriptor>>,
    /// Defining ID Set -- a set of all defining IDs on any types mentioned in the package.
    pub(crate) defining_ids: BTreeSet<DefiningTypeId>,
}

/// This is a lookup-only map for recording information about module members in loaded package
/// modules. It exposes an intentionally spartan interface to prevent any unexpected behavior
/// (e.g., unstable iteration ordering) that Rust's standard collections run afoul of.
#[derive(Debug)]
pub(crate) struct DefinitionMap<Value>(HashMap<IntraPackageKey, Value>);

/// original_address::module_name::function_name
/// NB: This relies on no boxing -- if this introduces boxes, the arena allocation in the execution
/// AST will leak memory.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
struct VirtualTableKey_ {
    package_key: OriginalId,
    inner_pkg_key: IntraPackageKey,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
#[repr(transparent)]
/// original_address::module_name::function_name
pub struct VirtualTableKey(VirtualTableKey_);

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) struct IntraPackageKey {
    pub(crate) module_name: IdentifierKey,
    pub(crate) member_name: IdentifierKey,
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
    /// The minimal depth of the value of this type regardless of type parameters.
    /// CBase
    pub constant: u64,
}

/// A formula for the layout size (in nodes) of a type: the linear analogue of [`DepthFormula`].
/// C + Σ Ni × Ti, where Ni is the number of times type parameter i occurs in the (expanded)
/// layout.
#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct LayoutFormula {
    /// The occurrence count for each type parameter, if present.
    pub(crate) terms: Vec<(TypeParameterIndex, u64)>,
    /// The layout nodes contributed regardless of type parameters.
    pub(crate) constant: u64,
}

/// The linkage-resolved through-field measure of a datatype (or of a type term over an
/// enclosing generic context), derived by folding the `DatatypeMeasure`s stored on descriptors
/// at JIT time: `value_depth` as a [`DepthFormula`] and `layout_size` as a [`LayoutFormula`],
/// both over type parameters.
#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct ResolvedMeasure {
    pub(crate) depth: DepthFormula,
    pub(crate) layout: LayoutFormula,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Clone for VMDispatchTables {
    fn clone(&self) -> Self {
        Self {
            vm_config: Arc::clone(&self.vm_config),
            interner: Arc::clone(&self.interner),
            loaded_packages: Arc::clone(&self.loaded_packages),
            defining_id_origins: Arc::clone(&self.defining_id_origins),
            link_context: Arc::clone(&self.link_context),
            type_measures: Mutex::new(self.type_measures().clone()),
        }
    }
}

// ------------------------------------------------------------------------
// The VM API that it will use to resolve packages and functions during execution of the
// transaction.
// ------------------------------------------------------------------------

impl VMDispatchTables {
    /// Create a new RuntimeVTables instance.
    /// NOTE: This assumes linkage has already occured.
    #[instrument(level = "trace", skip_all)]
    pub(crate) fn new(
        vm_config: Arc<VMConfig>,
        interner: Arc<IdentifierInterner>,
        link_context: LinkageContext,
        loaded_packages: BTreeMap<OriginalId, Arc<Package>>,
    ) -> VMResult<Self> {
        tracing::trace!(
            linkage_table = ?link_context,
            "creating VM dispatch tables"
        );
        let defining_id_origins = {
            let mut defining_id_map = BTreeMap::new();
            for (addr, pkg) in &loaded_packages {
                for defining_id in &pkg.vtable.defining_ids {
                    if let Some(prev) = defining_id_map.insert(*defining_id, *addr) {
                        return Err(partial_vm_error!(
                            UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            "Defining ID {defining_id} found for {addr} and {prev}"
                        )
                        .finish(Location::Package(pkg.version_id)));
                    }
                }
            }
            defining_id_map
        };

        let loaded_packages = Arc::new(loaded_packages);
        let defining_id_origins = Arc::new(defining_id_origins);
        let link_context = Arc::new(link_context);

        Ok(Self {
            vm_config,
            interner,
            loaded_packages,
            defining_id_origins,
            link_context,
            type_measures: Mutex::new(QCache::new(TYPE_DEPTH_LRU_SIZE)),
        })
    }

    pub fn get_package(&self, id: &OriginalId) -> PartialVMResult<Arc<Package>> {
        self.loaded_packages
            .get(id)
            .cloned()
            .ok_or_else(|| partial_vm_error!(VTABLE_KEY_LOOKUP_ERROR, "Package {} not found", id))
    }

    pub(crate) fn resolve_function(
        &self,
        vtable_key: &VirtualTableKey,
    ) -> PartialVMResult<VMPointer<Function>> {
        let Some(pkg) = self.loaded_packages.get(&vtable_key.0.package_key) else {
            return Err(partial_vm_error!(
                VTABLE_KEY_LOOKUP_ERROR,
                "Could not find package {}",
                vtable_key.0.package_key
            ));
        };
        if let Some(function_) = pkg.vtable.functions.get(&vtable_key.0.inner_pkg_key) {
            Ok(function_.ptr_clone())
        } else {
            Err(partial_vm_error!(
                VTABLE_KEY_LOOKUP_ERROR,
                "Could not find function {}",
                vtable_key.to_string(&self.interner)
            ))
        }
    }

    pub(crate) fn resolve_type(
        &self,
        vtable_key: &VirtualTableKey,
    ) -> PartialVMResult<VMPointer<DatatypeDescriptor>> {
        let Some(pkg) = self.loaded_packages.get(&vtable_key.0.package_key) else {
            return Err(partial_vm_error!(
                VTABLE_KEY_LOOKUP_ERROR,
                "Could not find package {}",
                vtable_key.0.package_key
            ));
        };
        if let Some(type_) = pkg.vtable.types.get(&vtable_key.0.inner_pkg_key) {
            Ok(type_.ptr_clone())
        } else {
            Err(partial_vm_error!(
                VTABLE_KEY_LOOKUP_ERROR,
                "Could not find type {}",
                vtable_key.to_string(&self.interner)
            ))
        }
    }

    /// This returns an `Option` instead of a `Result` because this is used in the function
    /// resolution logic for external calls, where failure to resolve should be treated as a "not
    /// found" instead of an error, and the caller will handle the erroring logic for "not found"
    /// cases.
    /// NB: It is important that this function return the same error (`None`) in the case where
    /// either:
    /// 1. The underlying identifiers were not found in the interner, or
    /// 2. the underlying package/module/function is not found.
    pub(super) fn try_resolve_function_for_external(
        &self,
        original_id: &ModuleId,
        function_name: &IdentStr,
    ) -> Option<VMPointer<Function>> {
        let vtable_key = self.try_to_virtual_table_key(
            original_id.address(),
            original_id.name(),
            function_name,
        )?;
        let pkg = self.loaded_packages.get(&vtable_key.0.package_key)?;
        let function_ = pkg.vtable.functions.get(&vtable_key.0.inner_pkg_key)?;
        Some(function_.ptr_clone())
    }

    /// This returns an `Option` instead of a `Result` because this is used in the type resolution
    /// logic for external calls into the VM, where failure to resolve should be treated as a "not
    /// found"/"failure to resolve" instead of an error, and the caller will handle the erroring
    /// logic for these cases.
    /// NB: It is important that this function return the same error (`None`) in the case where
    /// either:
    /// 1. The underlying identifiers were not found in the interner, or
    /// 2. the underlying package/module/type is not found.
    pub(super) fn try_resolve_type_for_external(
        &self,
        original_id: OriginalId,
        module_name: &IdentStr,
        type_name: &IdentStr,
    ) -> Option<(VMPointer<DatatypeDescriptor>, VirtualTableKey)> {
        let vtable_key = self.try_to_virtual_table_key(&original_id, module_name, type_name)?;
        let pkg = self.loaded_packages.get(&vtable_key.0.package_key)?;
        let type_ = pkg.vtable.types.get(&vtable_key.0.inner_pkg_key)?;
        Some((type_.ptr_clone(), vtable_key))
    }

    fn try_to_virtual_table_key(
        &self,
        package_id: &OriginalId,
        module: &IdentStr,
        name: &IdentStr,
    ) -> Option<VirtualTableKey> {
        let module_name = self.interner.get_ident_str(module)?;
        let member_name = self.interner.get_ident_str(name)?;
        Some(VirtualTableKey::from_parts(
            *package_id,
            module_name,
            member_name,
        ))
    }

    #[cfg(test)]
    pub(crate) fn to_virtual_table_key_for_testing(
        &self,
        original_id: &OriginalId,
        module: &IdentStr,
        name: &IdentStr,
    ) -> Option<VirtualTableKey> {
        self.try_to_virtual_table_key(original_id, module, name)
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
        self.load_type_impl(type_tag, &mut TypeSize::for_type_traversal())
            .map_err(|e| e.finish(Location::Undefined))
    }

    fn load_type_impl(
        &self,
        type_tag: &TypeTag,
        type_size: &mut TypeSize,
    ) -> PartialVMResult<Type> {
        type_size.enter_type(|type_size| {
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
                TypeTag::Vector(tt) => {
                    Type::Vector(Box::new(self.load_type_impl(tt, type_size)?))
                }
                // NB: Note that this tag is slightly misnamed and used for all Datatypes.
                TypeTag::Struct(struct_tag) => {
                    let defining_id = struct_tag.address;
                    let package_key =
                        *self.defining_id_origins.get(&defining_id).ok_or_else(|| {
                            partial_vm_error!(
                                EXTERNAL_RESOLUTION_REQUEST_ERROR,
                                "Defining ID {defining_id} for type {type_tag} not found in loaded packages"
                            )
                        })?;

                    let Some((datatype, key)) = self.try_resolve_type_for_external(
                        package_key,
                        &struct_tag.module,
                        &struct_tag.name,
                    ) else {
                        return Err(partial_vm_error!(
                            EXTERNAL_RESOLUTION_REQUEST_ERROR,
                            "Failed to resolve type for {type_tag} with package key {package_key} and defining ID {defining_id}"
                        ));
                    };

                    // The original ID on the datatype that we resolved should match the package
                    // key that we used to load it otherwise that's an invariant violation.
                    if datatype.original_id.address() != &package_key {
                        return Err(partial_vm_error!(
                            UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            "Runtime ID resolution of {defining_id} \
                                => {package_key} does not match runtime ID of loaded type: {}",
                            datatype.original_id.address()
                        ));
                    }
                    // The defining ID should match the defining ID of the datatype that we
                    // have loaded.
                    if datatype.defining_id.address() != &defining_id {
                        return Err(partial_vm_error!(
                            UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            "Defining ID {defining_id} does not match defining ID of loaded type: {}",
                            datatype.defining_id.address()
                        ));
                    }
                    if datatype.type_parameters().is_empty() && struct_tag.type_params.is_empty() {
                        Type::Datatype(key)
                    } else {
                        let mut type_params = vec![];
                        for ty_param in &struct_tag.type_params {
                            type_params.push(self.load_type_impl(ty_param, type_size)?);
                        }
                        self.verify_ty_args(datatype.type_param_constraints(), &type_params)?;
                        Type::DatatypeInstantiation(Box::new((key, type_params)))
                    }
                }
            })
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
            return Err(partial_vm_error!(NUMBER_OF_TYPE_ARGUMENTS_MISMATCH));
        }
        for (ty, expected_k) in ty_args.iter().zip(constraints) {
            if !expected_k.is_subset(self.abilities(ty)?) {
                return Err(partial_vm_error!(CONSTRAINT_NOT_SATISFIED));
            }
        }
        Ok(())
    }

    pub(crate) fn abilities(&self, ty: &Type) -> PartialVMResult<AbilitySet> {
        self.abilities_impl(ty, &mut TypeSize::for_type_traversal())
    }

    fn abilities_impl(&self, ty: &Type, type_size: &mut TypeSize) -> PartialVMResult<AbilitySet> {
        type_size.enter_type(|type_size| {
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

                Type::TyParam(_) => Err(partial_vm_error!(
                    UNREACHABLE,
                    "Unexpected TyParam type after translating from TypeTag to Type"
                )),

                Type::Vector(ty) => AbilitySet::polymorphic_abilities(
                    AbilitySet::VECTOR,
                    vec![false],
                    vec![self.abilities_impl(ty, type_size)?],
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
                        .map(|arg| self.abilities_impl(arg, type_size))
                        .collect::<PartialVMResult<Vec<_>>>()?;
                    AbilitySet::polymorphic_abilities(
                        *datatype_type.abilities(),
                        declared_phantom_parameters,
                        type_argument_abilities,
                    )
                }
            }
        })
    }

    pub(crate) fn datatype_information(&self, ty: &Type) -> PartialVMResult<Option<DatatypeInfo>> {
        Ok(match ty {
            Type::Bool
            | Type::U8
            | Type::U64
            | Type::U128
            | Type::Address
            | Type::Signer
            | Type::Vector(_)
            | Type::Reference(_)
            | Type::MutableReference(_)
            | Type::TyParam(_)
            | Type::U16
            | Type::U32
            | Type::U256 => None,
            Type::Datatype(vtable_key) => {
                let descriptor = self.resolve_type(vtable_key)?.to_ref();
                Some(DatatypeInfo {
                    original_id: *descriptor.original_id.address(),
                    defining_id: *descriptor.defining_id.address(),
                    module_name: descriptor.defining_id.name(&self.interner),
                    type_name: self.interner.resolve_ident(&descriptor.name, "type name"),
                })
            }
            Type::DatatypeInstantiation(inst) => {
                let (idx, _) = &**inst;
                let descriptor = self.resolve_type(idx)?.to_ref();
                Some(DatatypeInfo {
                    original_id: *descriptor.original_id.address(),
                    defining_id: *descriptor.defining_id.address(),
                    module_name: descriptor.defining_id.name(&self.interner),
                    type_name: self.interner.resolve_ident(&descriptor.name, "type name"),
                })
            }
        })
    }

    // -------------------------------------------
    // Through-Field Measure Computations (value depth and layout size)
    // -------------------------------------------
    // These functions derive the linkage-resolved through-field measure of datatypes and type
    // terms by folding the `DatatypeMeasure`s computed when each package was JIT'd: all purely
    // local field structure is already summarized in their constants, so the only recursion
    // left is into the datatype applications appearing in fields — each memoized in the cache.
    // Note that the `type_size` cursor consequently only charges for that residual work, not
    // for the local field structure the legacy traversals used to walk (and charge) here.

    /// Lock the measure cache, shrugging off poisoning: the cache is a pure optimization and
    /// its contents never affect correctness.
    fn type_measures(&self) -> std::sync::MutexGuard<'_, QCache<VirtualTableKey, ResolvedMeasure>> {
        self.type_measures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn cached_type_measure(&self, datatype: &VirtualTableKey) -> Option<ResolvedMeasure> {
        self.type_measures().get(datatype).cloned()
    }

    pub fn calculate_depth_of_type(
        &self,
        datatype_name: &VirtualTableKey,
    ) -> PartialVMResult<DepthFormula> {
        Ok(self
            .calculate_measure_of_datatype_and_cache(
                datatype_name,
                &mut TypeSize::from_vm_config_for_value_depth(&self.vm_config),
            )?
            .depth)
    }

    fn calculate_measure_of_datatype_and_cache(
        &self,
        datatype_name: &VirtualTableKey,
        type_size: &mut TypeSize,
    ) -> PartialVMResult<ResolvedMeasure> {
        type_size.check()?;
        // If we've already computed this datatype's measure, no more work remains to be done.
        if let Some(measure) = self.cached_type_measure(datatype_name) {
            return Ok(measure);
        }

        let datatype = self.resolve_type(datatype_name)?.to_ref();
        let measure = match datatype.datatype_measure() {
            // Fully concrete: the JIT wrote the sizes down.
            DatatypeMeasure::Constant(sizes) => ResolvedMeasure {
                depth: DepthFormula::constant(sizes.value_depth),
                layout: LayoutFormula::constant(sizes.layout_size),
            },
            DatatypeMeasure::Formula(datatype_formula) => {
                let mut depth_formulas = vec![DepthFormula::constant(
                    datatype_formula.value_depth_constant(),
                )];
                let mut layout = LayoutFormula::constant(datatype_formula.layout_size_constant());
                for term in datatype_formula.terms() {
                    let sub = match &term.var {
                        FieldVar::Param(idx) => ResolvedMeasure::type_parameter(*idx),
                        FieldVar::App(ty) => {
                            self.calculate_measure_of_type_and_cache(ty.to_ref(), type_size)?
                        }
                    };
                    let mut depth = sub.depth;
                    depth.add(term.depth_offset);
                    depth_formulas.push(depth);
                    layout.accumulate(term.occurrences, &sub.layout);
                }
                ResolvedMeasure {
                    depth: DepthFormula::normalize(depth_formulas),
                    layout,
                }
            }
        };
        // Insert without checking if it was already present; this is a pure optmization, so
        // we do not care about overwriting.
        self.type_measures()
            .insert(datatype_name.clone(), measure.clone());
        type_size.check()?;
        Ok(measure)
    }

    fn calculate_measure_of_type_and_cache(
        &self,
        ty: &ArenaType,
        type_size: &mut TypeSize,
    ) -> PartialVMResult<ResolvedMeasure> {
        type_size.enter_type(|type_size| {
            Ok(match ty {
                ArenaType::Bool
                | ArenaType::U8
                | ArenaType::U64
                | ArenaType::U128
                | ArenaType::Address
                | ArenaType::Signer
                | ArenaType::U16
                | ArenaType::U32
                | ArenaType::U256 => ResolvedMeasure::constant(1, 1),
                // we should not see the reference here, we could instead give an invariant
                // violation
                ArenaType::Vector(ty)
                | ArenaType::Reference(ty)
                | ArenaType::MutableReference(ty) => {
                    let mut inner = self.calculate_measure_of_type_and_cache(ty, type_size)?;
                    // add 1 for the vector itself
                    inner.depth.add(1);
                    inner.layout.constant = inner.layout.constant.saturating_add(1);
                    inner
                }
                ArenaType::TyParam(ty_idx) => ResolvedMeasure::type_parameter(*ty_idx),
                ArenaType::Datatype(datatype_key) => {
                    let datatype_measure =
                        self.calculate_measure_of_datatype_and_cache(datatype_key, type_size)?;
                    debug_assert!(datatype_measure.depth.terms.is_empty());
                    datatype_measure
                }
                ArenaType::DatatypeInstantiation(inst) => {
                    let (cache_idx, ty_args) = &**inst;
                    let arg_measures = ty_args
                        .iter()
                        .map(|ty| self.calculate_measure_of_type_and_cache(ty, type_size))
                        .collect::<PartialVMResult<Vec<_>>>()?;
                    let ty_arg_map = arg_measures
                        .iter()
                        .enumerate()
                        .map(|(idx, measure)| {
                            let var = checked_as!(idx, TypeParameterIndex)?;
                            Ok((var, measure.depth.clone()))
                        })
                        .collect::<PartialVMResult<BTreeMap<_, _>>>()?;
                    let datatype_measure =
                        self.calculate_measure_of_datatype_and_cache(cache_idx, type_size)?;

                    ResolvedMeasure {
                        depth: datatype_measure.depth.subst(ty_arg_map)?,
                        layout: datatype_measure.layout.subst(&arg_measures)?,
                    }
                }
            })
        })
    }

    /// The through-field pair (`value_depth`, `layout_size`) of a concrete runtime type,
    /// resolving datatypes through the measure cache.
    fn value_and_layout_of_type(
        &self,
        ty: &Type,
        type_size: &mut TypeSize,
    ) -> PartialVMResult<(u64, u64)> {
        type_size.enter_type(|type_size| {
            Ok(match ty {
                Type::Bool
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
                | Type::U128
                | Type::U256
                | Type::Address
                | Type::Signer => (1, 1),
                Type::Vector(inner) | Type::Reference(inner) | Type::MutableReference(inner) => {
                    let (value_depth, layout_size) =
                        self.value_and_layout_of_type(inner, type_size)?;
                    (value_depth.saturating_add(1), layout_size.saturating_add(1))
                }
                Type::Datatype(datatype_key) => {
                    let measure =
                        self.calculate_measure_of_datatype_and_cache(datatype_key, type_size)?;
                    (measure.depth.solve(&[])?, measure.layout.solve(&[])?)
                }
                Type::DatatypeInstantiation(inst) => {
                    let (datatype_key, ty_args) = &**inst;
                    let mut value_depths = Vec::with_capacity(ty_args.len());
                    let mut layout_sizes = Vec::with_capacity(ty_args.len());
                    for ty_arg in ty_args {
                        let (value_depth, layout_size) =
                            self.value_and_layout_of_type(ty_arg, type_size)?;
                        value_depths.push(value_depth);
                        layout_sizes.push(layout_size);
                    }
                    let measure =
                        self.calculate_measure_of_datatype_and_cache(datatype_key, type_size)?;
                    (
                        measure.depth.solve(&value_depths)?,
                        measure.layout.solve(&layout_sizes)?,
                    )
                }
                Type::TyParam(_) => {
                    return Err(partial_vm_error!(
                        UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        "Type parameter should be fully resolved"
                    ));
                }
            })
        })
    }

    /// All four size quantities of a concrete runtime type.
    pub(crate) fn sizes_of_type(&self, ty: &Type) -> PartialVMResult<TypeSizes> {
        let TypeMeasure {
            type_size,
            type_depth,
        } = ty.measure();
        let (value_depth, layout_size) = self.value_and_layout_of_type(
            ty,
            &mut TypeSize::from_vm_config_for_value_depth(&self.vm_config),
        )?;
        Ok(TypeSizes {
            type_size,
            type_depth,
            value_depth,
            layout_size,
        })
    }

    /// Pair fully-instantiated type arguments with their sizes, computed once here so a call
    /// frame can carry them.
    pub(crate) fn make_type_arguments(&self, types: Vec<Type>) -> PartialVMResult<TypeArguments> {
        TypeArguments::new(types, |ty| self.sizes_of_type(ty))
    }

    /// Check the `value_depth` of instantiating `term` with `ty_args` against the configured
    /// limit, without building anything: the term's formula is solved with the frame-cached
    /// argument value depths. Used at `VecPack`, where a value of the instantiated element type
    /// is about to be created.
    pub(crate) fn check_instantiated_value_depth(
        &self,
        term: &ArenaType,
        ty_args: &TypeArguments,
    ) -> PartialVMResult<()> {
        let Some(max_depth) = self.vm_config.runtime_limits_config.max_value_nest_depth else {
            return Ok(());
        };
        let measure = self.calculate_measure_of_type_and_cache(
            term,
            &mut TypeSize::from_vm_config_for_value_depth(&self.vm_config),
        )?;
        let value_depths = ty_args
            .sizes()
            .iter()
            .map(|sizes| sizes.value_depth)
            .collect::<Vec<_>>();
        let value_depth = measure.depth.solve(&value_depths)?;
        if value_depth > max_depth {
            return Err(partial_vm_error!(VM_MAX_VALUE_DEPTH_REACHED));
        }
        Ok(())
    }

    // -------------------------------------------
    // Type Translation Helpers
    // -------------------------------------------

    fn datatype_to_type_tag_impl(
        &self,
        datatype_name: &VirtualTableKey,
        ty_args: &[Type],
        tag_type: DatatypeTagType,
        type_size: &mut TypeSize,
    ) -> PartialVMResult<StructTag> {
        type_size.check()?;
        let type_params = ty_args
            .iter()
            .map(|ty| self.type_to_type_tag_impl(ty, tag_type, type_size))
            .collect::<PartialVMResult<Vec<_>>>()?;
        let datatype = self.resolve_type(datatype_name)?.to_ref();

        let (address, module) = match tag_type {
            DatatypeTagType::Runtime => (
                *datatype.original_id.address(),
                datatype.original_id.name(&self.interner).to_owned(),
            ),

            DatatypeTagType::Defining => (
                *datatype.defining_id.address(),
                datatype.defining_id.name(&self.interner).to_owned(),
            ),
        };
        let name = self.interner.resolve_ident(&datatype.name, "datatype name");

        let tag = StructTag {
            address,
            module,
            name,
            type_params,
        };
        type_size.check()?;
        Ok(tag)
    }

    fn type_to_type_tag_impl(
        &self,
        ty: &Type,
        tag_type: DatatypeTagType,
        type_size: &mut TypeSize,
    ) -> PartialVMResult<TypeTag> {
        type_size.enter_type(|type_size| {
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
                Type::Vector(ty) => TypeTag::Vector(Box::new(
                    self.type_to_type_tag_impl(ty, tag_type, type_size)?,
                )),
                Type::Datatype(gidx) => TypeTag::Struct(Box::new(self.datatype_to_type_tag_impl(
                    gidx,
                    &[],
                    tag_type,
                    type_size,
                )?)),
                Type::DatatypeInstantiation(struct_inst) => {
                    let (gidx, ty_args) = &**struct_inst;
                    TypeTag::Struct(Box::new(
                        self.datatype_to_type_tag_impl(gidx, ty_args, tag_type, type_size)?,
                    ))
                }
                Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                    return Err(partial_vm_error!(
                        UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        "no type tag for {:?}",
                        ty
                    ));
                }
            })
        })
    }

    pub(crate) fn type_to_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_to_type_tag_impl(
            ty,
            DatatypeTagType::Defining,
            &mut TypeSize::for_type_traversal(),
        )
    }

    pub(crate) fn type_to_runtime_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_to_type_tag_impl(
            ty,
            DatatypeTagType::Runtime,
            &mut TypeSize::for_type_traversal(),
        )
    }

    /// Check a type's predicted `value_depth` and `layout_size` against the configured limits
    /// before any layout generation — pure arithmetic over the descriptor measures; nothing of
    /// an oversized layout is ever built. The error codes mirror the legacy cursor's.
    fn check_layout_limits(&self, ty: &Type) -> PartialVMResult<()> {
        let (value_depth, layout_size) = self.value_and_layout_of_type(
            ty,
            &mut TypeSize::from_vm_config_for_value_depth(&self.vm_config),
        )?;
        if value_depth
            > self
                .vm_config
                .runtime_limits_config
                .max_value_nest_depth
                .unwrap_or(VALUE_DEPTH_MAX)
        {
            return Err(partial_vm_error!(VM_MAX_TYPE_DEPTH_REACHED));
        }
        if layout_size
            > self
                .vm_config
                .max_type_to_layout_nodes
                .unwrap_or(HISTORICAL_MAX_TYPE_TO_LAYOUT_NODES)
        {
            return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
        }
        Ok(())
    }

    pub(crate) fn type_to_type_layout(
        &self,
        ty: &Type,
    ) -> PartialVMResult<runtime_value::MoveTypeLayout> {
        // The layout builders do not enforce limits themselves: `check_layout_limits` below
        // bounds their recursion. They are local to this function so the check can never be
        // bypassed.
        fn datatype_to_type_layout(
            tables: &VMDispatchTables,
            datatype_name: &VirtualTableKey,
            ty_args: &[Type],
        ) -> PartialVMResult<runtime_value::MoveDatatypeLayout> {
            let ty = tables.resolve_type(datatype_name)?.to_ref();
            let type_layout = match ty.datatype_info.inner_ref() {
                Datatype::Enum(einfo) => {
                    let mut variant_layouts = vec![];
                    for variant in einfo.variants.iter() {
                        let field_tys = variant
                            .fields
                            .iter()
                            .map(|ty| ty.subst(ty_args))
                            .collect::<PartialVMResult<Vec<_>>>()?;
                        let field_layouts = field_tys
                            .iter()
                            .map(|ty| type_to_type_layout_unchecked(tables, ty))
                            .collect::<PartialVMResult<Vec<_>>>()?;
                        variant_layouts.push(field_layouts);
                    }
                    runtime_value::MoveDatatypeLayout::Enum(Box::new(
                        runtime_value::MoveEnumLayout(Box::new(variant_layouts)),
                    ))
                }
                Datatype::Struct(sinfo) => {
                    let field_tys = sinfo
                        .fields
                        .iter()
                        .map(|ty| ty.subst(ty_args))
                        .collect::<PartialVMResult<Vec<_>>>()?;
                    let field_layouts = field_tys
                        .iter()
                        .map(|ty| type_to_type_layout_unchecked(tables, ty))
                        .collect::<PartialVMResult<Vec<_>>>()?;

                    runtime_value::MoveDatatypeLayout::Struct(Box::new(
                        runtime_value::MoveStructLayout::new(field_layouts),
                    ))
                }
            };
            Ok(type_layout)
        }

        fn type_to_type_layout_unchecked(
            tables: &VMDispatchTables,
            ty: &Type,
        ) -> PartialVMResult<runtime_value::MoveTypeLayout> {
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
                    type_to_type_layout_unchecked(tables, ty)?,
                )),
                Type::Datatype(gidx) => datatype_to_type_layout(tables, gidx, &[])?.into_layout(),
                Type::DatatypeInstantiation(inst) => {
                    let (gidx, ty_args) = &**inst;
                    datatype_to_type_layout(tables, gidx, ty_args)?.into_layout()
                }
                Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                    return Err(partial_vm_error!(
                        UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        "no type layout for {:?}",
                        ty
                    ));
                }
            };
            Ok(result)
        }

        self.check_layout_limits(ty)?;
        type_to_type_layout_unchecked(self, ty)
    }

    pub(crate) fn arena_type_to_fully_annotated_layout(
        &self,
        ty: &ArenaType,
    ) -> PartialVMResult<annotated_value::MoveTypeLayout> {
        self.type_to_fully_annotated_layout(&ty.to_type()?)
    }

    pub(crate) fn type_to_fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> PartialVMResult<annotated_value::MoveTypeLayout> {
        // The layout builders do not enforce limits themselves: `check_layout_limits` below
        // bounds their recursion. They are local to this function so the check can never be
        // bypassed.
        fn datatype_to_fully_annotated_layout(
            tables: &VMDispatchTables,
            datatype_name: &VirtualTableKey,
            ty_args: &[Type],
        ) -> PartialVMResult<annotated_value::MoveDatatypeLayout> {
            let ty = tables.resolve_type(datatype_name)?.to_ref();
            let struct_tag = tables.datatype_to_type_tag_impl(
                datatype_name,
                ty_args,
                DatatypeTagType::Defining,
                &mut TypeSize::for_type_traversal(),
            )?;

            let type_layout = match ty.datatype_info.inner_ref() {
                Datatype::Enum(enum_type) => {
                    let mut variant_layouts = BTreeMap::new();
                    for variant in enum_type.variants.iter() {
                        if variant.fields.len() != variant.field_names.len() {
                            return Err(partial_vm_error!(
                                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                                "Field types did not match the length of field names in loaded enum variant"
                            ));
                        }
                        let field_layouts = variant
                            .field_names
                            .iter()
                            .zip(variant.fields.iter())
                            .map(|(n, ty)| {
                                let n = tables.interner.resolve_ident(n, "field name");
                                let ty = ty.subst(ty_args)?;
                                let l = type_to_fully_annotated_layout_unchecked(tables, &ty)?;
                                Ok(annotated_value::MoveFieldLayout::new(n, l))
                            })
                            .collect::<PartialVMResult<Vec<_>>>()?;
                        variant_layouts.insert(
                            (
                                tables
                                    .interner
                                    .resolve_ident(&variant.variant_name, "variant name"),
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
                        return Err(partial_vm_error!(
                            UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            "Field types did not match the length of field names in loaded struct"
                        ));
                    }
                    let field_layouts = struct_type
                        .field_names
                        .iter()
                        .zip(struct_type.fields.iter())
                        .map(|(n, ty)| {
                            let n = tables.interner.resolve_ident(n, "field name");
                            let ty = ty.subst(ty_args)?;
                            let l = type_to_fully_annotated_layout_unchecked(tables, &ty)?;
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

        fn type_to_fully_annotated_layout_unchecked(
            tables: &VMDispatchTables,
            ty: &Type,
        ) -> PartialVMResult<annotated_value::MoveTypeLayout> {
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
                    type_to_fully_annotated_layout_unchecked(tables, ty)?,
                )),
                Type::Datatype(gidx) => {
                    datatype_to_fully_annotated_layout(tables, gidx, &[])?.into_layout()
                }
                Type::DatatypeInstantiation(inst) => {
                    let (gidx, ty_args) = &**inst;
                    datatype_to_fully_annotated_layout(tables, gidx, ty_args)?.into_layout()
                }
                Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                    return Err(partial_vm_error!(
                        UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        "no type layout for {:?}",
                        ty
                    ));
                }
            };
            Ok(result)
        }

        self.check_layout_limits(ty)?;
        type_to_fully_annotated_layout_unchecked(self, ty)
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

    pub(crate) fn instantiate_generic_function(
        &self,
        fun_inst: &FunctionInstantiation,
        type_params: &TypeArguments,
    ) -> PartialVMResult<TypeArguments> {
        // Each element is checked against the limits via its precomputed formula before it is
        // built.
        let instantiation = fun_inst
            .instantiation
            .to_ref()
            .iter()
            .map(|ty| ty.instantiate(type_params))
            .collect::<PartialVMResult<Vec<_>>>()?;
        // The callee frame's type arguments: computing their sizes here, once, is what makes
        // every later check against them pure arithmetic.
        let instantiation = self.make_type_arguments(instantiation)?;

        // Check if the function instantiation over all generics is larger
        // than the max instantiation node count.
        // Pure arithmetic: all the sizes involved are already computed.
        let max_nodes = MAX_TYPE_INSTANTIATION_NODES;
        let mut sum_nodes = 1u64;
        for sizes in type_params
            .sizes()
            .iter()
            .chain(instantiation.sizes().iter())
        {
            sum_nodes = sum_nodes.saturating_add(sizes.type_size);
            if sum_nodes > max_nodes {
                return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
            }
        }
        Ok(instantiation)
    }

    pub(crate) fn instantiate_single_type(
        &self,
        ty: &FormulatedType,
        ty_args: &TypeArguments,
    ) -> PartialVMResult<Type> {
        if ty_args.is_empty() {
            ty.to_type()
        } else {
            ty.instantiate(ty_args)
        }
    }

    pub(crate) fn instantiate_struct_type(
        &self,
        struct_inst: &StructInstantiation,
        ty_args: &TypeArguments,
    ) -> PartialVMResult<Type> {
        let type_params = struct_inst.type_params.to_ref();
        self.instantiate_datatype_common(&struct_inst.def_vtable_key, type_params, ty_args)
    }

    pub(crate) fn instantiate_enum_type(
        &self,
        variant_inst: &VariantInstantiation,
        ty_args: &TypeArguments,
    ) -> PartialVMResult<Type> {
        let enum_inst = variant_inst.enum_inst.to_ref();
        let type_params = enum_inst.type_params.to_ref();
        self.instantiate_datatype_common(&enum_inst.def_vtable_key, type_params, ty_args)
    }

    fn instantiate_datatype_common(
        &self,
        datatype_key: &VirtualTableKey,
        type_params: &[FormulatedType],
        ty_args: &TypeArguments,
    ) -> PartialVMResult<Type> {
        // Before instantiating the type, count the # of nodes of all type arguments plus
        // existing type instantiation.
        // If that number is larger than the max instantiation node count, refuse to
        // construct this type.
        // This prevents constructing larger and larger types via datatype instantiation.
        // Pure arithmetic: the term sizes are the stored formula constants and the argument
        // sizes were measured when the `TypeArguments` were built.
        let max_nodes = MAX_TYPE_INSTANTIATION_NODES;
        let mut sum_nodes = 1u64;
        for nodes in type_params
            .iter()
            .map(|ty| ty.measure().type_size)
            .chain(ty_args.sizes().iter().map(|sizes| sizes.type_size))
        {
            sum_nodes = sum_nodes.saturating_add(nodes);
            if sum_nodes > max_nodes {
                return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
            }
        }

        // Build the type-parameter instantiations, each checked against the limits via its
        // precomputed formula, while accumulating the result's true node count arithmetically
        // (the prediction counts the parameter nodes themselves in addition to the substituted
        // arguments; see `MeasureFormula`).
        let mut result_nodes = 1u64;
        let mut instantiation = Vec::with_capacity(type_params.len());
        for ty in type_params.iter() {
            instantiation.push(ty.instantiate(ty_args)?);
            let child_nodes = ty
                .predict(ty_args)?
                .type_size
                .saturating_sub(ty.occurrences());
            result_nodes = result_nodes.saturating_add(child_nodes);
        }
        let ty = Type::DatatypeInstantiation(Box::new((datatype_key.clone(), instantiation)));

        if result_nodes > MAX_TYPE_INSTANTIATION_NODES {
            return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
        }

        // A value of this datatype is about to be created (this path serves the `Pack` family
        // of instructions), so also check its `value_depth` — the third quantity — predicted
        // from the datatype's measure, the type-parameter terms, and the frame-cached argument
        // value depths. Nothing is traversed: the built type is never walked.
        if let Some(max_depth) = self.vm_config.runtime_limits_config.max_value_nest_depth {
            let cursor = &mut TypeSize::from_vm_config_for_value_depth(&self.vm_config);
            let param_depths = type_params
                .iter()
                .map(|term| {
                    let measure = self.calculate_measure_of_type_and_cache(term.ty(), cursor)?;
                    let value_depths = ty_args
                        .sizes()
                        .iter()
                        .map(|sizes| sizes.value_depth)
                        .collect::<Vec<_>>();
                    measure.depth.solve(&value_depths)
                })
                .collect::<PartialVMResult<Vec<_>>>()?;
            let datatype_measure =
                self.calculate_measure_of_datatype_and_cache(datatype_key, cursor)?;
            if datatype_measure.depth.solve(&param_depths)? > max_depth {
                return Err(partial_vm_error!(VM_MAX_VALUE_DEPTH_REACHED));
            }
        }

        Ok(ty)
    }
}

impl DepthFormula {
    /// A value with no type parameters
    pub fn constant(constant: u64) -> Self {
        Self {
            terms: vec![],
            constant,
        }
    }

    /// A stand alone type parameter value
    pub fn type_parameter(tparam: TypeParameterIndex) -> Self {
        Self {
            terms: vec![(tparam, 0)],
            constant: 0,
        }
    }

    /// We `max` over a list of formulas, and we normalize it to deal with duplicate terms, e.g.
    /// `max(max(t1 + 1, t2 + 2, 2), max(t1 + 3, t2 + 1, 4))` becomes
    /// `max(t1 + 3, t2 + 2, 4)`
    pub fn normalize(formulas: Vec<Self>) -> Self {
        let mut var_map = BTreeMap::new();
        let mut constant_acc = 0;
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
            constant_acc = std::cmp::max(constant_acc, constant);
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
        let mut formulas = vec![DepthFormula::constant(*constant)];
        for (t_i, c_i) in terms {
            let Some(mut u_form) = map.remove(t_i) else {
                return Err(partial_vm_error!(
                    UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    "{t_i:?} missing mapping"
                ));
            };
            u_form.add(*c_i);
            formulas.push(u_form)
        }
        Ok(DepthFormula::normalize(formulas))
    }

    /// Given depths for each type parameter, solve the formula giving the max depth for the type
    pub fn solve(&self, tparam_depths: &[u64]) -> PartialVMResult<u64> {
        let Self { terms, constant } = self;
        let mut depth = *constant;
        for (t_i, c_i) in terms {
            match tparam_depths.get(*t_i as usize) {
                None => {
                    return Err(partial_vm_error!(
                        UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        "{t_i:?} missing mapping"
                    ));
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
        *constant = (*constant).saturating_add(c);
    }
}

impl LayoutFormula {
    /// A layout with no type parameters.
    pub(crate) fn constant(constant: u64) -> Self {
        Self {
            terms: vec![],
            constant,
        }
    }

    /// A single type parameter occurring once.
    pub(crate) fn type_parameter(tparam: TypeParameterIndex) -> Self {
        Self {
            terms: vec![(tparam, 1)],
            constant: 0,
        }
    }

    /// Add `occurrences` copies of `other` into this formula.
    pub(crate) fn accumulate(&mut self, occurrences: u64, other: &LayoutFormula) {
        self.constant = self
            .constant
            .saturating_add(occurrences.saturating_mul(other.constant));
        for (t_i, n_i) in &other.terms {
            let scaled = occurrences.saturating_mul(*n_i);
            match self.terms.iter_mut().find(|(t_j, _)| t_j == t_i) {
                Some((_, n_j)) => *n_j = n_j.saturating_add(scaled),
                None => self.terms.push((*t_i, scaled)),
            }
        }
    }

    /// Substitute in formulas for each type parameter (indexed positionally by `arg_measures`).
    pub(crate) fn subst(&self, arg_measures: &[ResolvedMeasure]) -> PartialVMResult<LayoutFormula> {
        let mut result = LayoutFormula::constant(self.constant);
        for (t_i, n_i) in &self.terms {
            let Some(arg) = arg_measures.get(*t_i as usize) else {
                return Err(partial_vm_error!(
                    UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    "{t_i:?} missing mapping"
                ));
            };
            result.accumulate(*n_i, &arg.layout);
        }
        Ok(result)
    }

    /// C + Σ Ni × Ti with the given per-parameter layout sizes.
    pub(crate) fn solve(&self, tparam_layouts: &[u64]) -> PartialVMResult<u64> {
        let Self { terms, constant } = self;
        let mut layout_size = *constant;
        for (t_i, n_i) in terms {
            match tparam_layouts.get(*t_i as usize) {
                None => {
                    return Err(partial_vm_error!(
                        UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        "{t_i:?} missing mapping"
                    ));
                }
                Some(ty_layout) => {
                    layout_size = layout_size.saturating_add(n_i.saturating_mul(*ty_layout))
                }
            }
        }
        Ok(layout_size)
    }
}

impl ResolvedMeasure {
    /// A fully concrete measure.
    pub(crate) fn constant(value_depth: u64, layout_size: u64) -> Self {
        Self {
            depth: DepthFormula::constant(value_depth),
            layout: LayoutFormula::constant(layout_size),
        }
    }

    /// The measure of a bare type parameter.
    pub(crate) fn type_parameter(tparam: TypeParameterIndex) -> Self {
        Self {
            depth: DepthFormula::type_parameter(tparam),
            layout: LayoutFormula::type_parameter(tparam),
        }
    }
}

// ------------------------------------------------------------------------
// Other Impls
// ------------------------------------------------------------------------

impl PackageVirtualTable {
    pub(crate) fn new(
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
                return Err(partial_vm_error!(
                    UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    "Duplicate vtable key"
                ));
            }
        }
        Ok(())
    }

    /// Retrieve a key from the definition map.
    pub fn get(&self, key: &IntraPackageKey) -> Option<&T> {
        self.0.get(key)
    }
}

impl VirtualTableKey {
    /// [SAFETY] This cannot be ade public, because it may lead to panics in the interner.
    pub(crate) fn from_parts(
        package_key: OriginalId,
        module_name: IdentifierKey,
        member_name: IdentifierKey,
    ) -> Self {
        let inner = VirtualTableKey_ {
            package_key,
            inner_pkg_key: IntraPackageKey {
                module_name,
                member_name,
            },
        };
        VirtualTableKey(inner)
    }

    /// [SAFETY] This cannot be ade public, because it may lead to panics in the interner.
    pub(crate) fn intra_package_key(&self) -> &IntraPackageKey {
        &self.0.inner_pkg_key
    }

    /// [SAFETY] This cannot be ade public, because it may lead to panics in the interner.
    pub(crate) fn package_key(&self) -> OriginalId {
        self.0.package_key
    }

    pub fn module_id(&self, interner: &IdentifierInterner) -> ModuleId {
        let module_name = interner.resolve_ident(&self.0.inner_pkg_key.module_name, "module name");
        ModuleId::new(self.0.package_key, module_name)
    }

    pub fn member_name(&self, interner: &IdentifierInterner) -> Identifier {
        interner.resolve_ident(&self.0.inner_pkg_key.member_name, "member name")
    }

    pub fn to_string(&self, interner: &IdentifierInterner) -> String {
        let inner_name = self.0.inner_pkg_key.to_string(interner);
        format!(
            "{}::{}",
            self.0
                .package_key
                .to_canonical_display(/* with_prefix */ true),
            inner_name
        )
    }

    pub fn to_short_string(&self, interner: &IdentifierInterner) -> String {
        let inner_name = self.0.inner_pkg_key.to_string(interner);
        format!(
            "0x{}::{}",
            self.0.package_key.short_str_lossless(),
            inner_name,
        )
    }
}

impl IntraPackageKey {
    pub fn to_string(self, interner: &IdentifierInterner) -> String {
        let module_name = interner.resolve_ident(&self.module_name, "module name");
        let member_name = interner.resolve_ident(&self.member_name, "member name");
        format!("{}::{}", module_name, member_name)
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
