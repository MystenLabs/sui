// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{
    linkage::resolved_linkage::ExecutableLinkage,
    loading::ast::{LoadedFunction, Type},
};
use indexmap::IndexSet;
use move_core_types::{
    account_address::AccountAddress, identifier::IdentStr, language_storage::TypeTag,
};
use move_vm_runtime::execution as vm_runtime;
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    rc::Rc,
};
use sui_protocol_config::ProtocolConfig;
use sui_types::{Identifier, base_types::ObjectID, error::ExecutionError, type_input::TypeInput};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct TypeLinkageCacheKey {
    // NB: We use a BTreeSet here to ensure that the order of the root IDs does not affect the
    // cache key.
    root_ids: BTreeSet<AccountAddress>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct LoadedFunctionKey {
    package: ObjectID,
    module: Identifier,
    function: Identifier,
    type_arguments: Vec<Type>,
}

pub(super) struct PerTxCache<'pc> {
    protocol_config: &'pc ProtocolConfig,
    inner: RefCell<PerTxCache_>,
}

/// Per-transaction memoization tables shared across the PTB pipeline.
///
/// The tables fall into four groups:
///
/// 1. **Linkage resolution** (`type_linkage_cache`): computing an `ExecutableLinkage` for a set of
///    defining-ID addresses touches every transitive dependency of those packages; the same set
///    of roots may be queried many times within a single PTB (once per type input, once per type
///    argument on a call, etc.). This caches that lookup.
///
/// 2. **TypeInput resolution** (`type_input_to_type`): user-supplied `TypeInput`s are resolved to
///    adapter `Type`s via a tag lookup + VM load. Kept one-way because multiple `TypeInput`s can
///    resolve to the same `Type` so a reverse map would be ambiguous.
///
/// 3. **Bijective type conversions** (`tag_to_type` / `type_to_tag`, `vm_to_type` / `type_to_vm`):
///    adapter `Type`, `TypeTag`, and `vm_runtime::Type` all carry defining IDs, so the mappings
///    between them are one-to-one for fully-resolved types. Each bijection is kept as two maps
///    sharing `Rc`-owned value sides, and populated atomically through a single helper
///    (`insert_tag_type_pair`, `insert_vm_type_pair`) so the two directions cannot drift.
///
/// 4. **Loaded function resolution** (`function_cache`): resolving a Move call to a
///    `LoadedFunction` involves call-linkage computation, VM instantiation against that linkage,
///    function-def lookup, and type-parameter substitution. The full result is a pure function
///    of `(version-specific package, module, function, type arguments)`, so we cache the
///    `Rc<LoadedFunction>` keyed on that tuple.
///
/// All fields are `HashMap`-backed: the keys are either structural type trees or defining-ID
/// tuples, so hashing dominates over ordered iteration, and `vm_runtime::Type` in particular
/// only implements `Hash + Eq` (not `Ord`).
struct PerTxCache_ {
    /// Linkage resolutions keyed by `<defining-ID root set>`.
    ///
    /// NB: this can only be used for determining type linkages.
    type_linkage_cache: HashMap<TypeLinkageCacheKey, ExecutableLinkage>,

    /// TypeInput -> adapter Type. One-way only.
    type_input_to_type: HashMap<TypeInput, Type>,

    /// Bijective Type <-> TypeTag. Populated atomically via `insert_tag_type_pair`.
    tag_to_type: HashMap<Rc<TypeTag>, Type>,
    type_to_tag: HashMap<Type, Rc<TypeTag>>,

    /// Bijective Type <-> vm_runtime::Type. Only fully-resolved types (no `TyParam`). Populated
    /// atomically via `insert_vm_type_pair`.
    vm_to_type: HashMap<Rc<vm_runtime::Type>, Type>,
    type_to_vm: HashMap<Type, Rc<vm_runtime::Type>>,

    /// `(package version-id, module, function, type arguments)` -> `Rc<LoadedFunction>`. Keyed on
    /// the version-specific package ID because private/entry functions can disappear across
    /// package versions, so different versions resolve differently.
    function_cache: HashMap<LoadedFunctionKey, Rc<LoadedFunction>>,

    defining_id_map: HashMap<(ObjectID, Identifier, Identifier), ObjectID>,
}

/// Early-returns `Ok($empty)` from the enclosing method when PTB caching is disabled. The second
/// argument is the stand-in result for that short-circuit: `None` for lookups, `()` for inserts,
/// or a freshly-allocated `(Rc, value)` pair for the paired inserts that must still hand a valid
/// return back to the caller.
macro_rules! gated {
    ($config:expr, $empty:expr) => {
        if !$config.enable_ptb_tx_cache() {
            return Ok($empty);
        }
    };
}

impl<'pc> PerTxCache<'pc> {
    pub(super) fn new(protocol_config: &'pc ProtocolConfig) -> Self {
        Self {
            protocol_config,
            inner: RefCell::new(PerTxCache_ {
                type_linkage_cache: HashMap::new(),
                type_input_to_type: HashMap::new(),
                tag_to_type: HashMap::new(),
                type_to_tag: HashMap::new(),
                vm_to_type: HashMap::new(),
                type_to_vm: HashMap::new(),
                function_cache: HashMap::new(),
                defining_id_map: HashMap::new(),
            }),
        }
    }

    fn borrow(&self) -> Result<std::cell::Ref<'_, PerTxCache_>, ExecutionError> {
        self.inner.try_borrow().map_err(|_| {
            make_invariant_violation!(
                "Should be able to borrow PerTxCache for access here as we are only accessing it"
            )
        })
    }

    fn borrow_mut(&self) -> Result<std::cell::RefMut<'_, PerTxCache_>, ExecutionError> {
        self.inner.try_borrow_mut().map_err(|_| {
            make_invariant_violation!(
                "Should be able to borrow PerTxCache for mutation here as we are only mutating it"
            )
        })
    }

    pub(super) fn lookup_type_linkage(
        &self,
        key: &TypeLinkageCacheKey,
    ) -> Result<Option<ExecutableLinkage>, ExecutionError> {
        gated!(self.protocol_config, None);
        Ok(self.borrow()?.type_linkage_cache.get(key).cloned())
    }

    pub(super) fn insert_type_linkage(
        &self,
        key: TypeLinkageCacheKey,
        linkage: ExecutableLinkage,
    ) -> Result<(), ExecutionError> {
        gated!(self.protocol_config, ());
        self.borrow_mut()?.type_linkage_cache.insert(key, linkage);
        Ok(())
    }

    pub(super) fn lookup_type_input(
        &self,
        input: &TypeInput,
    ) -> Result<Option<Type>, ExecutionError> {
        gated!(self.protocol_config, None);
        Ok(self.borrow()?.type_input_to_type.get(input).cloned())
    }

    pub(super) fn insert_type_input(
        &self,
        input: TypeInput,
        ty: Type,
    ) -> Result<(), ExecutionError> {
        gated!(self.protocol_config, ());
        assert_invariant!(
            !matches!(ty, Type::Reference(_, _)),
            "TypeInput cannot represent a reference type: {:?}",
            ty
        );

        let previous = self.borrow_mut()?.type_input_to_type.insert(input, ty);
        assert_invariant!(
            previous.is_none(),
            "duplicate insert into type_input_to_type"
        );

        Ok(())
    }

    pub(super) fn lookup_type_by_tag(&self, tag: &TypeTag) -> Result<Option<Type>, ExecutionError> {
        gated!(self.protocol_config, None);
        Ok(self.borrow()?.tag_to_type.get(tag).cloned())
    }

    pub(super) fn lookup_tag(&self, ty: &Type) -> Result<Option<Rc<TypeTag>>, ExecutionError> {
        gated!(self.protocol_config, None);
        Ok(self.borrow()?.type_to_tag.get(ty).cloned())
    }

    pub(super) fn insert_tag_type_pair(
        &self,
        tag: TypeTag,
        ty: Type,
    ) -> Result<(Rc<TypeTag>, Type), ExecutionError> {
        let tag = Rc::new(tag);

        gated!(self.protocol_config, (tag, ty));
        assert_invariant!(
            !matches!(ty, Type::Reference(_, _)),
            "TypeTag cannot represent a reference type: {:?}",
            ty
        );

        let mut c = self.borrow_mut()?;

        let previous_tag = c.tag_to_type.insert(tag.clone(), ty.clone());
        assert_invariant!(previous_tag.is_none(), "duplicate insert into tag_to_type");

        let previous_type = c.type_to_tag.insert(ty.clone(), tag.clone());
        assert_invariant!(previous_type.is_none(), "duplicate insert into type_to_tag");

        Ok((tag, ty))
    }

    pub(super) fn lookup_type_by_vm_type(
        &self,
        vm_type: &vm_runtime::Type,
    ) -> Result<Option<Type>, ExecutionError> {
        gated!(self.protocol_config, None);
        Ok(self.borrow()?.vm_to_type.get(vm_type).cloned())
    }

    pub(super) fn lookup_vm_type(
        &self,
        ty: &Type,
    ) -> Result<Option<Rc<vm_runtime::Type>>, ExecutionError> {
        gated!(self.protocol_config, None);
        Ok(self.borrow()?.type_to_vm.get(ty).cloned())
    }

    pub(super) fn insert_vm_type_pair(
        &self,
        vm_type: vm_runtime::Type,
        ty: Type,
    ) -> Result<(Rc<vm_runtime::Type>, Type), ExecutionError> {
        let vm_type = Rc::new(vm_type);

        gated!(self.protocol_config, (vm_type, ty));
        assert_invariant!(
            !matches!(vm_type.as_ref(), vm_runtime::Type::TyParam(_)),
            "cannot cache unresolved TyParam: {:?}",
            vm_type
        );

        let mut c = self.borrow_mut()?;

        let previous_vm_type = c.vm_to_type.insert(vm_type.clone(), ty.clone());
        assert_invariant!(
            previous_vm_type.is_none(),
            "duplicate insert into vm_to_type"
        );

        let previous_type = c.type_to_vm.insert(ty.clone(), vm_type.clone());
        assert_invariant!(previous_type.is_none(), "duplicate insert into type_to_vm");

        Ok((vm_type, ty))
    }

    pub(super) fn lookup_function(
        &self,
        key: &LoadedFunctionKey,
    ) -> Result<Option<Rc<LoadedFunction>>, ExecutionError> {
        gated!(self.protocol_config, None);
        Ok(self.borrow()?.function_cache.get(key).cloned())
    }

    pub(super) fn insert_function(
        &self,
        key: LoadedFunctionKey,
        function: Rc<LoadedFunction>,
    ) -> Result<(), ExecutionError> {
        gated!(self.protocol_config, ());
        let previous_function = self.borrow_mut()?.function_cache.insert(key, function);
        assert_invariant!(
            previous_function.is_none(),
            "duplicate insert into function_cache"
        );
        Ok(())
    }

    pub(super) fn insert_type_to_defining_id(
        &self,
        package: ObjectID,
        module: &IdentStr,
        name: &IdentStr,
        defining_id: ObjectID,
    ) -> Result<(), ExecutionError> {
        gated!(self.protocol_config, ());
        let previous = self
            .borrow_mut()?
            .defining_id_map
            .insert((package, module.to_owned(), name.to_owned()), defining_id);
        assert_invariant!(
            previous.is_none(),
            "duplicate insert into defining_id_map: ({}::{}::{})",
            package,
            module,
            name
        );
        Ok(())
    }

    pub(super) fn lookup_type_to_defining_id(
        &self,
        package: ObjectID,
        module: &IdentStr,
        name: &IdentStr,
    ) -> Result<Option<ObjectID>, ExecutionError> {
        gated!(self.protocol_config, None);
        Ok(self
            .borrow()?
            .defining_id_map
            .get(&(package, module.to_owned(), name.to_owned()))
            .cloned())
    }
}

impl TypeLinkageCacheKey {
    pub(super) fn new(root_ids: &IndexSet<AccountAddress>) -> Self {
        Self {
            root_ids: root_ids.iter().copied().collect(),
        }
    }
}

impl LoadedFunctionKey {
    pub(super) fn new(
        package: ObjectID,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<Type>,
    ) -> Self {
        Self {
            package,
            module,
            function,
            type_arguments,
        }
    }
}
