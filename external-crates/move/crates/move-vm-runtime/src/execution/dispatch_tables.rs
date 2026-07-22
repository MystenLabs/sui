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
        ArenaType, Datatype, DatatypeDescriptor, Function, FunctionInstantiation, Package,
        StructInstantiation, Type, VariantInstantiation,
    },
    shared::{
        TraversalBudget,
        constants::{
            HISTORICAL_MAX_TYPE_TO_LAYOUT_NODES, MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_LRU_SIZE,
            VALUE_DEPTH_MAX,
        },
        linkage_context::LinkageContext,
        type_size_formulae::{PartialTypeSizeFormula, TypeSize, Visiting, check_syntactic_limits},
        types::{DefiningTypeId, OriginalId},
        vm_pointer::VMPointer,
    },
};

use move_binary_format::{
    errors::{Location, PartialVMResult, VMResult},
    file_format::AbilitySet,
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
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::Deref,
    sync::Arc,
};

/// A per-execution cache of datatypes' resolved size formulae, keyed by datatype under the
/// enclosing resolver's linkage view.
///
/// This is deliberately an *unsynchronized* cache. It lives only on a [`VMDispatchTables`]
/// resolver, which is constructed per transaction and never shared across threads (only the
/// `Sync` [`DispatchTables`] it wraps is cached and shared). The interpreter and the natives are
/// therefore free to fill it in place through `&self`.
pub(crate) struct TypeCache(RefCell<QCache<VirtualTableKey, PartialTypeSizeFormula>>);

impl std::fmt::Debug for TypeCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TypeCache({} entries)", self.0.borrow().len())
    }
}

impl TypeCache {
    fn new() -> Self {
        Self(RefCell::new(QCache::new(TYPE_DEPTH_LRU_SIZE)))
    }

    fn get(&self, key: &VirtualTableKey) -> Option<PartialTypeSizeFormula> {
        self.0.borrow().get(key).cloned()
    }

    fn insert(&self, key: VirtualTableKey, formula: PartialTypeSizeFormula) {
        self.0.borrow_mut().insert(key, formula);
    }
}

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// The shared, `Sync` resolution tables: everything the VM needs to resolve packages, functions,
/// and datatypes under a fixed linkage. Built once per linkage and cached (and cloned) across
/// transactions — the fields are all `Arc`s, so cloning is cheap.
///
/// This holds no per-execution state, which is what keeps it `Sync` and safe to share across the
/// worker threads that read the package cache. Per-execution state (the size-formula cache) lives
/// on the [`VMDispatchTables`] resolver that wraps this.
#[derive(Debug, Clone)]
pub struct DispatchTables {
    pub(crate) vm_config: Arc<VMConfig>,
    pub(crate) interner: Arc<IdentifierInterner>,
    pub(crate) loaded_packages: Arc<BTreeMap<OriginalId, Arc<Package>>>,
    /// Defining ID Set -- a set of all defining IDs on any types defined in the package.
    /// [SAFETY] Ordering is not guaranteed
    pub(crate) defining_id_origins: Arc<BTreeMap<DefiningTypeId, OriginalId>>,
    pub(crate) link_context: Arc<LinkageContext>,
}

/// The per-execution dispatch resolver: the shared [`DispatchTables`] plus a transaction-local
/// [`TypeCache`] of resolved size formulae.
///
/// This is a transient (transaction-scoped) value: it is created at the beginning of a transaction
/// from the (cached) `Sync` tables and dropped at the end. Because it is never shared across
/// threads, its size cache can be unsynchronized. It derefs to [`DispatchTables`], so all the
/// resolution methods read the shared tables directly.
#[derive(Debug)]
pub struct VMDispatchTables {
    pub(crate) tables: DispatchTables,
    pub(crate) size_formulas: TypeCache,
}

impl Deref for VMDispatchTables {
    type Target = DispatchTables;

    fn deref(&self) -> &DispatchTables {
        &self.tables
    }
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

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl DispatchTables {
    /// Create the shared resolution tables for a linkage.
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

        Ok(Self {
            vm_config,
            interner,
            loaded_packages: Arc::new(loaded_packages),
            defining_id_origins: Arc::new(defining_id_origins),
            link_context: Arc::new(link_context),
        })
    }
}

// ------------------------------------------------------------------------
// The VM API that it will use to resolve packages and functions during execution of the
// transaction.
// ------------------------------------------------------------------------

impl VMDispatchTables {
    /// Wrap shared resolution tables in a fresh per-execution resolver (with an empty size cache).
    pub(crate) fn new(tables: DispatchTables) -> Self {
        Self {
            tables,
            size_formulas: TypeCache::new(),
        }
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
        self.load_type_impl(type_tag, &mut TraversalBudget::for_type_traversal())
            .map_err(|e| e.finish(Location::Undefined))
    }

    fn load_type_impl(
        &self,
        type_tag: &TypeTag,
        type_size_budget: &mut TraversalBudget,
    ) -> PartialVMResult<Type> {
        type_size_budget.enter_type(|type_size_budget| {
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
                    Type::Vector(Box::new(self.load_type_impl(tt, type_size_budget)?))
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
                            type_params.push(self.load_type_impl(ty_param, type_size_budget)?);
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
        self.abilities_impl(ty, &mut TraversalBudget::for_type_traversal())
    }

    fn abilities_impl(
        &self,
        ty: &Type,
        type_size_budget: &mut TraversalBudget,
    ) -> PartialVMResult<AbilitySet> {
        type_size_budget.enter_type(|type_size_budget| {
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
                    vec![self.abilities_impl(ty, type_size_budget)?],
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
                        .map(|arg| self.abilities_impl(arg, type_size_budget))
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
    // Type sizing
    // -------------------------------------------
    // Type sizing works via partial evaluation / application. The vtable is our translation's
    // `env`, and `partial_type_size` translates a datatype's JIT-built `ArenaTypeSizeFormula`
    // against this linkage into a flat `PartialTypeSizeFormula` over the datatype's parameters,
    // memoized per key in `size_formulas`. `substitute` recurs back through it, so every datatype
    // boundary re-enters the cache, optimizing this approach. Finally, `type_size_of` walks a
    // concrete runtime type straight to a `TypeSize` from this information.

    /// The resolved formula of a datatype under this linkage, over the datatype's own parameters.
    /// `visiting` turns a cyclic (corrupt/adversarial) datatype graph into an invariant violation.
    fn partial_type_size_impl(
        &self,
        key: &VirtualTableKey,
        visiting: &mut Visiting,
    ) -> PartialVMResult<PartialTypeSizeFormula> {
        if let Some(hit) = self.size_formulas.get(key) {
            return Ok(hit);
        }
        if !visiting.insert(key.clone()) {
            return Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "cyclic datatype definition encountered while resolving size formula for {}",
                key.to_string(&self.interner)
            ));
        }
        let descriptor = self.resolve_type(key)?;
        // `size_formula()` is arena-resident (VMPointer), so it does not borrow `self`.
        let formula = descriptor
            .to_ref()
            .size_formula()
            .substitute(self, visiting)?;
        visiting.remove(key);
        self.size_formulas.insert(key.clone(), formula.clone());
        Ok(formula)
    }

    /// The resolved formula of a datatype under this linkage.
    pub(crate) fn partial_type_size(
        &self,
        key: &VirtualTableKey,
    ) -> PartialVMResult<PartialTypeSizeFormula> {
        if let Some(hit) = self.size_formulas.get(key) {
            return Ok(hit);
        }
        self.partial_type_size_impl(key, &mut Visiting::new())
    }

    pub(crate) fn size_formula_impl(
        &self,
        ty: &ArenaType,
        visiting: &mut Visiting,
    ) -> PartialVMResult<PartialTypeSizeFormula> {
        Ok(match ty {
            ArenaType::TyParam(idx) => PartialTypeSizeFormula::parameter(*idx),
            ArenaType::Vector(inner)
            | ArenaType::Reference(inner)
            | ArenaType::MutableReference(inner) => self.size_formula_impl(inner, visiting)?.wrap(),
            ArenaType::Datatype(key) => self.partial_type_size_impl(key, visiting)?,
            ArenaType::DatatypeInstantiation(inst) => {
                let (key, args) = &**inst;
                let arg_forms = args
                    .iter()
                    .map(|arg| self.size_formula_impl(arg, visiting))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                self.partial_type_size_impl(key, visiting)?
                    .substitute(&arg_forms)?
            }
            ArenaType::Bool
            | ArenaType::U8
            | ArenaType::U16
            | ArenaType::U32
            | ArenaType::U64
            | ArenaType::U128
            | ArenaType::U256
            | ArenaType::Address
            | ArenaType::Signer => PartialTypeSizeFormula::primitive(),
        })
    }

    /// Interpret an arena type term into its size formula over the ambient (function)
    /// parameters.
    /// Note: datatype nodes resolve through the cache.
    pub(crate) fn size_formula(&self, ty: &ArenaType) -> PartialVMResult<PartialTypeSizeFormula> {
        self.size_formula_impl(ty, &mut Visiting::new())
    }

    /// The four sizes of a concrete runtime type, assuming no free parameters. Datatype nodes
    /// resolve through the cache.
    pub(crate) fn type_size_of(&self, ty: &Type) -> PartialVMResult<TypeSize> {
        Ok(match ty {
            Type::Vector(inner) | Type::Reference(inner) | Type::MutableReference(inner) => {
                TypeSize::wrap(self.type_size_of(inner)?)
            }
            Type::Datatype(key) => self.partial_type_size(key)?.solve(&[])?,
            Type::DatatypeInstantiation(inst) => {
                let (key, args) = &**inst;
                let arg_sizes = args
                    .iter()
                    .map(|arg| self.type_size_of(arg))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                self.partial_type_size(key)?.solve(&arg_sizes)?
            }
            Type::TyParam(_) => {
                return Err(partial_vm_error!(
                    UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    "Type parameter should be fully resolved"
                ));
            }
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::U256
            | Type::Address
            | Type::Signer => TypeSize::PRIMITIVE,
        })
    }

    /// Check a computed value's `value_depth` against the configured limit (if any).
    fn check_value_depth(&self, value_depth: u64) -> PartialVMResult<()> {
        if let Some(max) = self.vm_config.runtime_limits_config.max_value_nest_depth
            && value_depth > max
        {
            return Err(partial_vm_error!(VM_MAX_VALUE_DEPTH_REACHED));
        }
        Ok(())
    }

    /// Check the limits of a `vector<elem>` value about to be created at `VecPack`, without
    /// building the type. As at any creation site, the element type's syntactic size and depth
    /// are checked (this is the only place the interpreter validates a vector's element type;
    /// the other vector ops operate on already-validated vectors), along with the created value's
    /// nesting depth — the `+ 1` accounts for the vector wrapping the element value. All three
    /// come from the one formula solve.
    pub(crate) fn check_vector_element(
        &self,
        elem: &ArenaType,
        ty_args: &[(Type, TypeSize)],
    ) -> PartialVMResult<()> {
        let arg_sizes: Vec<TypeSize> = ty_args.iter().map(|(_, size)| *size).collect();
        let elem_size = self.size_formula(elem)?.solve(&arg_sizes)?;
        check_syntactic_limits(elem_size.type_size, elem_size.type_depth)?;
        self.check_value_depth(elem_size.value_depth.saturating_add(1))
    }

    // -------------------------------------------
    // Type Translation Helpers
    // -------------------------------------------

    fn datatype_to_type_tag_impl(
        &self,
        datatype_name: &VirtualTableKey,
        ty_args: &[Type],
        tag_type: DatatypeTagType,
        type_size_budget: &mut TraversalBudget,
    ) -> PartialVMResult<StructTag> {
        type_size_budget.check()?;
        let type_params = ty_args
            .iter()
            .map(|ty| self.type_to_type_tag_impl(ty, tag_type, type_size_budget))
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
        type_size_budget.check()?;
        Ok(tag)
    }

    fn type_to_type_tag_impl(
        &self,
        ty: &Type,
        tag_type: DatatypeTagType,
        type_size_budget: &mut TraversalBudget,
    ) -> PartialVMResult<TypeTag> {
        type_size_budget.enter_type(|type_size_budget| {
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
                Type::Vector(ty) => TypeTag::Vector(Box::new(self.type_to_type_tag_impl(
                    ty,
                    tag_type,
                    type_size_budget,
                )?)),
                Type::Datatype(gidx) => TypeTag::Struct(Box::new(self.datatype_to_type_tag_impl(
                    gidx,
                    &[],
                    tag_type,
                    type_size_budget,
                )?)),
                Type::DatatypeInstantiation(struct_inst) => {
                    let (gidx, ty_args) = &**struct_inst;
                    TypeTag::Struct(Box::new(self.datatype_to_type_tag_impl(
                        gidx,
                        ty_args,
                        tag_type,
                        type_size_budget,
                    )?))
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
            &mut TraversalBudget::for_type_traversal(),
        )
    }

    pub(crate) fn type_to_runtime_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_to_type_tag_impl(
            ty,
            DatatypeTagType::Runtime,
            &mut TraversalBudget::for_type_traversal(),
        )
    }

    /// Check a type's predicted `value_depth` and `layout_size` against the configured limits
    /// before any layout generation — pure arithmetic over the descriptor formulas; nothing of
    /// an oversized layout is ever built. The error codes mirror the legacy cursor's.
    fn check_layout_limits(&self, ty: &Type) -> PartialVMResult<()> {
        let size = self.type_size_of(ty)?;
        if size.value_depth
            > self
                .vm_config
                .runtime_limits_config
                .max_value_nest_depth
                .unwrap_or(VALUE_DEPTH_MAX)
        {
            return Err(partial_vm_error!(VM_MAX_TYPE_DEPTH_REACHED));
        }
        if size.layout_size
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
                            .map(|ty| ty.subst_unchecked(ty_args))
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
                        .map(|ty| ty.subst_unchecked(ty_args))
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
                &mut TraversalBudget::for_type_traversal(),
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
                                let ty = ty.subst_unchecked(ty_args)?;
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
                            let ty = ty.subst_unchecked(ty_args)?;
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

    /// The one checked substitution primitive: realize `term` against `ty_args` and return both
    /// the built type and its size, having first checked the size against the syntactic limits.
    /// Every route to a substituted type goes through here.
    fn realize_type(
        &self,
        term: &ArenaType,
        ty_args: &[(Type, TypeSize)],
    ) -> PartialVMResult<(Type, TypeSize)> {
        let arg_sizes: Vec<TypeSize> = ty_args.iter().map(|(_, size)| *size).collect();
        let size = self.size_formula(term)?.solve(&arg_sizes)?;
        check_syntactic_limits(size.type_size, size.type_depth)?;
        let arg_types: Vec<Type> = ty_args.iter().map(|(ty, _)| ty.clone()).collect();
        Ok((term.subst_unchecked(&arg_types)?, size))
    }

    /// Realize a generic function's callee type arguments and measure each one, producing the
    /// `(Type, TypeSize)` pairs the callee frame is built from. Every size is solved from the
    /// term's formula against the caller frame's argument sizes — the realized type is never
    /// walked for measurement — and each term is checked against the syntactic limits and the
    /// running instantiation-node budget before it is built.
    pub(crate) fn instantiate_generic_function(
        &self,
        fun_inst: &FunctionInstantiation,
        ty_args: &[(Type, TypeSize)],
    ) -> PartialVMResult<Vec<(Type, TypeSize)>> {
        let terms = fun_inst.instantiation.to_ref();
        let mut result = Vec::with_capacity(terms.len());

        // The whole instantiation — caller arguments plus the callee arguments realized below —
        // must fit within the instantiation-node budget. Pure arithmetic over the sizes.
        let mut sum_nodes = 1u64;
        for (_, size) in ty_args.iter() {
            sum_nodes = sum_nodes.saturating_add(size.type_size);
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
            }
        }

        for term in terms.iter() {
            let (ty, size) = self.realize_type(term, ty_args)?;
            sum_nodes = sum_nodes.saturating_add(size.type_size);
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
            }
            result.push((ty, size));
        }
        Ok(result)
    }

    /// Realize a single term with `ty_args`, checking its predicted syntactic sizes against the
    /// limits first — the type is built only if it fits.
    pub(crate) fn subst_type(
        &self,
        term: &ArenaType,
        ty_args: &[(Type, TypeSize)],
    ) -> PartialVMResult<Type> {
        Ok(self.realize_type(term, ty_args)?.0)
    }

    /// Check a struct instantiation (the `Pack` family of instructions) against the limits
    /// without building anything — see [`VMDispatchTables::check_instantiation`].
    pub(crate) fn check_struct_instantiation(
        &self,
        struct_inst: &StructInstantiation,
        ty_args: &[(Type, TypeSize)],
    ) -> PartialVMResult<()> {
        self.check_instantiation(
            &struct_inst.def_vtable_key,
            struct_inst.type_params.to_ref(),
            ty_args,
        )
    }

    /// Check an enum variant instantiation (the `PackVariant` family of instructions) against
    /// the limits without building anything — see [`VMDispatchTables::check_instantiation`].
    pub(crate) fn check_variant_instantiation(
        &self,
        variant_inst: &VariantInstantiation,
        ty_args: &[(Type, TypeSize)],
    ) -> PartialVMResult<()> {
        let enum_inst = variant_inst.enum_inst.to_ref();
        self.check_instantiation(
            &enum_inst.def_vtable_key,
            enum_inst.type_params.to_ref(),
            ty_args,
        )
    }

    /// Realize a struct instantiation's runtime type, checking the instantiation as a value of it
    /// is about to exist. Observational (the tracer).
    pub(crate) fn instantiate_struct_type(
        &self,
        struct_inst: &StructInstantiation,
        ty_args: &[(Type, TypeSize)],
    ) -> PartialVMResult<Type> {
        self.check_struct_instantiation(struct_inst, ty_args)?;
        self.instantiate_datatype_type(
            &struct_inst.def_vtable_key,
            struct_inst.type_params.to_ref(),
            ty_args,
        )
    }

    /// Realize an enum instantiation's runtime type, checking the instantiation as a value of it
    /// is about to exist. Observational (the tracer).
    pub(crate) fn instantiate_enum_type(
        &self,
        variant_inst: &VariantInstantiation,
        ty_args: &[(Type, TypeSize)],
    ) -> PartialVMResult<Type> {
        self.check_variant_instantiation(variant_inst, ty_args)?;
        let enum_inst = variant_inst.enum_inst.to_ref();
        self.instantiate_datatype_type(
            &enum_inst.def_vtable_key,
            enum_inst.type_params.to_ref(),
            ty_args,
        )
    }

    fn instantiate_datatype_type(
        &self,
        datatype_key: &VirtualTableKey,
        type_params: &[ArenaType],
        ty_args: &[(Type, TypeSize)],
    ) -> PartialVMResult<Type> {
        let instantiation = type_params
            .iter()
            .map(|term| self.subst_type(term, ty_args))
            .collect::<PartialVMResult<Vec<_>>>()?;
        Ok(Type::DatatypeInstantiation(Box::new((
            datatype_key.clone(),
            instantiation,
        ))))
    }

    /// Check a datatype instantiation against the limits without realizing anything: the
    /// type-node counts and the result value's `value_depth` are all solved from precomputed
    /// formulas. This path serves the `Pack` family of instructions — the instantiated type
    /// itself is never needed, so no part of it is ever built.
    fn check_instantiation(
        &self,
        datatype_key: &VirtualTableKey,
        type_params: &[ArenaType],
        ty_args: &[(Type, TypeSize)],
    ) -> PartialVMResult<()> {
        let arg_sizes: Vec<TypeSize> = ty_args.iter().map(|(_, size)| *size).collect();
        // Realize each type-parameter term's size against the frame's argument sizes, checking
        // each against the syntactic limits as it is computed. Nothing is built.
        let mut param_sizes = Vec::with_capacity(type_params.len());
        for term in type_params.iter() {
            let size = self.size_formula(term)?.solve(&arg_sizes)?;
            check_syntactic_limits(size.type_size, size.type_depth)?;
            param_sizes.push(size);
        }

        // A value of this datatype is about to be created. Solve the datatype's resolved formula
        // against the realized parameter sizes for the result's *true* node/depth counts, and
        // check them: bounding the constructed type's node count is what stops types growing
        // without bound through repeated instantiation. Its `value_depth` is checked too, since a
        // value is created. Note this is the exact size of the built type — the running-argument
        // sum a naive guard would use over-counts an argument once per parameter that references
        // it (the `S<T×32>` blow-up: 128 arguments-plus-parameters, but 3041 realized nodes).
        let result = self.partial_type_size(datatype_key)?.solve(&param_sizes)?;
        check_syntactic_limits(result.type_size, result.type_depth)?;
        self.check_value_depth(result.value_depth)?;
        Ok(())
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
