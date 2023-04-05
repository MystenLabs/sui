// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, bail, Result};
use move_binary_format::normalized::{Function, Type};
use move_bytecode_utils::{
    layout::TypeLayoutBuilder,
    module_cache::{GetModule, ModuleCache},
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, ResourceKey, TypeTag},
    resolver::MoveResolver,
    value::MoveValue,
};
use move_read_write_set_types::{Access, AccessPath, Offset, ReadWriteSet, RootAddress};
use std::{
    borrow::Borrow,
    fmt::{self, Formatter},
    ops::Deref,
};

/// A read/write set state with no unbound formals or type variables
#[derive(Debug)]
pub struct ConcretizedFormals(ReadWriteSet);

/// A read/write set state with no secondary indexes and no unbound formals or type variables
#[derive(Debug)]
pub struct ConcretizedSecondaryIndexes(ConcretizedFormals);

impl ConcretizedFormals {
    /// Return a `Self` that accesses nothing.
    pub fn empty() -> Self {
        Self(ReadWriteSet::new())
    }

    /// Return the `ResourceKey`'s that may be written by `self`.
    /// For example: if `self` is 0x7/0x1::AModule::AResource/f/g -> ReadWrite, this will return
    /// 0x7/0x1::AModule.
    pub fn get_keys_written(&self) -> Option<Vec<ResourceKey>> {
        self.0.get_keys_written()
    }

    /// Return the `ResourceKey`'s read by `self`.
    pub fn get_keys_read(&self) -> Option<Vec<ResourceKey>> {
        self.0.get_keys_read()
    }

    /// Concretize all secondary indexes in `self` using `blockchain_view` and return the result. For
    /// example: if `self` is 0x7/0x1::AModule::AResource/addr_field/0x2::M2::R/f -> Write and the
    /// value of 0x7/0x1::AModule::AResource/addr_field is 0xA in `blockchain_view`, this will
    /// return { 0x7/0x1::AModule::AResource/addr_field -> Read, 0xA/0x2::M2::R/f -> Write }
    pub fn concretize_secondary_indexes<R: MoveResolver>(
        self,
        blockchain_view: &R,
    ) -> Option<ConcretizedSecondaryIndexes> {
        // TODO: check if there are no secondary indexes and return accesses if so
        let mut acc = ReadWriteSet::new();
        let module_cache = ModuleCache::new(blockchain_view);
        self.0.iter_paths(|access_path, access| {
            Self::concretize_secondary_indexes_(
                &module_cache,
                blockchain_view,
                access_path,
                access,
                &mut acc,
            )
            .ok()
        })?;
        Some(ConcretizedSecondaryIndexes(ConcretizedFormals(acc)))
    }

    /// Construct a `ConcretizedFormals` from `accesses` by binding the formals and type variables in
    /// `accesses` to `actuals` and `type_actuals`.
    fn from_args_(
        read_write_set: &ReadWriteSet,
        actuals: &[Option<AccountAddress>],
        type_actuals: &[TypeTag],
    ) -> ConcretizedFormals {
        ConcretizedFormals(read_write_set.sub_actuals(actuals, type_actuals))
    }

    /// Construct a `ConcretizedFormals` from `accesses` by binding the formals and type variables in
    /// `accesses` to `signers`/`actuals` and `type_actuals`.
    /// For example: if `accesses` is Formal(0)/0x1::M::S<TypeVar(0)>/f -> Read, `signers` is 0xA
    /// and type_actuals is 0x2::M2::S2, this will return  0xA/0x1::M::S<0x2::M2::S@>/f -> Read
    pub fn from_args(
        read_write_set: &ReadWriteSet,
        signers: &[AccountAddress],
        actuals: &[Vec<u8>],
        formal_types: &[TypeTag],
        type_actuals: &[TypeTag],
    ) -> Result<ConcretizedFormals> {
        let mut new_actuals = signers
            .iter()
            .map(|addr| Some(*addr))
            .collect::<Vec<Option<_>>>();
        assert_eq!(
            formal_types.len(),
            actuals.len() + signers.len(),
            "Formal/actual arity mismatch"
        );
        // Deserialize the address actuals, use an None for the rest
        let num_signers = signers.len();
        // formal_types includes all formal types (including signers), but actuals is only
        // the non-signer actuals.
        for (actual_index, ty) in formal_types[num_signers..].iter().enumerate() {
            let actual = if let TypeTag::Address = ty {
                Some(AccountAddress::from_bytes(&actuals[actual_index])?)
            } else {
                None
            };
            new_actuals.push(actual);
        }
        Ok(Self::from_args_(
            read_write_set,
            new_actuals.as_slice(),
            type_actuals,
        ))
    }

    /// Concretize the secondary in `offsets` -> `access` using `annotator` and add the results to
    /// `acc`.
    fn concretize_offsets<R: MoveResolver>(
        module_cache: &ModuleCache<&R>,
        blockchain_view: &R,
        access_path: AccessPath,
        mut next_value: MoveValue,
        next_offset_index: usize,
        access: &Access,
        acc: &mut ReadWriteSet,
    ) -> Result<()> {
        let offsets = access_path.offset();
        for next_offset_index in next_offset_index..offsets.len() {
            let next_offset = &offsets[next_offset_index];
            match next_offset {
                Offset::Field(index) => {
                    if let MoveValue::Struct(s) = next_value {
                        let fields = s.fields();
                        next_value = fields[*index].clone();
                    } else {
                        bail!("Malformed access path {:?}; expected struct value as prefix to field offset {:?}, but got {:?}",
                               access_path, next_offset, next_value)
                    }
                }
                Offset::Global(g_offset) => {
                    // secondary index. previous offset should have been an address value
                    if let MoveValue::Address(a) = next_value {
                        let mut new_ap = AccessPath::new_global_constant(a, g_offset.clone());
                        for o in offsets[next_offset_index + 1..].iter() {
                            new_ap.add_offset(o.clone())
                        }
                        return Self::concretize_secondary_indexes_(
                            module_cache,
                            blockchain_view,
                            &new_ap,
                            access,
                            acc,
                        );
                    } else {
                        bail!(
                            "Malformed access path {:?}: expected address value before Global offset, but found {:?}",
                            access_path,
                            next_value
                        );
                    }
                }
                Offset::VectorIndex => {
                    if let MoveValue::Vector(v_contents) = &next_value {
                        // concretize offsets for each element in the vector
                        for val in v_contents {
                            Self::concretize_offsets(
                                module_cache,
                                blockchain_view,
                                access_path.clone(),
                                val.clone(),
                                next_offset_index + 1,
                                access,
                                acc,
                            )?;
                        }
                        return Ok(());
                    } else {
                        bail!(
                            "Malformed access path {:?}: expected vector value before VectorIndex offset, but found {:?}",
                            access_path,
                            next_value
                        );
                    }
                }
            }
        }
        // Got to the end of the concrete access path. Add it to the accumulator
        acc.add_access_path(access_path, *access);

        Ok(())
    }

    /// Concretize the secondary indexes in `access_path` and add the result to `acc`. For example
    fn concretize_secondary_indexes_<R: MoveResolver>(
        module_cache: &ModuleCache<&R>,
        blockchain_view: &R,
        access_path: &AccessPath,
        access: &Access,
        acc: &mut ReadWriteSet,
    ) -> Result<()> {
        if let RootAddress::Const(g) = &access_path.root.root {
            let tag = access_path
                .root
                .type_
                .clone()
                .into_struct_tag()
                .ok_or_else(|| {
                    anyhow!("Unbound type variable found: {:?}", access_path.root.type_)
                })?;
            if let Some(resource_bytes) = blockchain_view
                .get_resource(g, &tag)
                .map_err(|_| anyhow!("Failed to get resource for {:?}::{:?}", g, tag))?
            {
                let layout =
                    TypeLayoutBuilder::build_runtime(&TypeTag::Struct(Box::new(tag)), module_cache)
                        .map_err(|_| {
                            anyhow!("Failed to resolve type: {:?}", access_path.root.type_)
                        })?;

                let resource =
                    MoveValue::simple_deserialize(&resource_bytes, &layout).map_err(|_| {
                        anyhow!(
                            "Failed to deserialize move value of type: {:?}",
                            access_path.root.type_
                        )
                    })?;
                Self::concretize_offsets(
                    module_cache,
                    blockchain_view,
                    access_path.clone(),
                    resource,
                    0,
                    access,
                    acc,
                )?;
            } // else, resource not present. this can happen/is expected because the R/W set is overapproximate
        }
        Ok(())
    }
}

/// Bind all formals and type variables in `accesses` using `signers`, `actuals`, and
/// `type_actuals`.
pub fn bind_formals<R: GetModule>(
    accesses: &ReadWriteSet,
    module: &ModuleId,
    fun: &IdentStr,
    signers: &[AccountAddress],
    actuals: &[Vec<u8>],
    type_actuals: &[TypeTag],
    module_cache: &R,
) -> Result<ConcretizedFormals> {
    let subst_map = type_actuals
        .iter()
        .map(|ty| Type::from(ty.clone()))
        .collect::<Vec<_>>();

    let compiled_module = module_cache
        .get_module_by_id(module)
        .map_err(|_| anyhow!("Failed to get module from storage"))?
        .ok_or_else(|| anyhow!("Failed to get module"))?;

    let func_sig = Function::new_from_name(compiled_module.borrow(), fun)
        .ok_or_else(|| anyhow!("Failed to find function"))?;

    // Check arity before binding. Otherwise we might get out-of-bound errors.
    if func_sig.parameters.len() != actuals.len() + signers.len()
        || func_sig.type_parameters.len() != type_actuals.len()
    {
        bail!("Script arity doesn't match");
    }

    let func_type = func_sig
        .parameters
        .iter()
        .map(|ty| ty.subst(&subst_map).into_type_tag())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow!("Failed to substitute types"))?;

    ConcretizedFormals::from_args(
        accesses,
        signers,
        actuals,
        func_type.as_slice(),
        type_actuals,
    )
}

/// Bind all formals and type variables in `accesses` using `signers`, `actuals`, and
/// `type_actuals`. In addition, concretize all secondary indexes in `accesses` against the state in
/// `blockchain_view`.
pub fn concretize(
    accesses: &ReadWriteSet,
    module: &ModuleId,
    fun: &IdentStr,
    signers: &[AccountAddress],
    actuals: &[Vec<u8>],
    type_actuals: &[TypeTag],
    blockchain_view: &impl MoveResolver,
) -> Result<ConcretizedSecondaryIndexes> {
    let module_cache = ModuleCache::new(blockchain_view);
    bind_formals(
        accesses,
        module,
        fun,
        signers,
        actuals,
        type_actuals,
        &module_cache,
    )?
    .concretize_secondary_indexes(blockchain_view)
    .ok_or_else(|| anyhow!("Failed to concretize secondary index"))
}

impl Deref for ConcretizedFormals {
    type Target = ReadWriteSet;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for ConcretizedSecondaryIndexes {
    type Target = ConcretizedFormals;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for ConcretizedFormals {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.0)
    }
}

impl fmt::Display for ConcretizedSecondaryIndexes {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.0)
    }
}
