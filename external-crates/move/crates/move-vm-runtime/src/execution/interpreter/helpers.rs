// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Execution Operation  Helpers
// -------------------------------------------------------------------------------------------------
// These functions perform type substitution, instantiation, etc.
// Historically, these were part of some resolver -- now, the AST's pointers can be chased
// directly, broadly obviating the need for such a resolver.

use crate::{
    execution::dispatch_tables::VirtualTableKey,
    jit::execution::ast::{
        ArenaType, FunctionInstantiation, StructInstantiation, Type, TypeNodeCount, TypeSubst,
        VariantInstantiation,
    },
    shared::constants::MAX_TYPE_INSTANTIATION_NODES,
};

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;

pub fn instantiate_generic_function(
    fun_inst: &FunctionInstantiation,
    type_params: &[Type],
) -> PartialVMResult<Vec<Type>> {
    let instantiation: Vec<_> = fun_inst
        .instantiation
        .to_ref()
        .iter()
        .map(|ty| ty.subst(type_params))
        .collect::<PartialVMResult<_>>()?;

    // Check if the function instantiation over all generics is larger
    // than MAX_TYPE_INSTANTIATION_NODES.
    let mut sum_nodes = 1u64;
    for ty in type_params.iter().chain(instantiation.iter()) {
        sum_nodes = sum_nodes.saturating_add(ty.count_type_nodes());
        if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
            return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
        }
    }
    Ok(instantiation)
}

pub fn instantiate_single_type(ty: &ArenaType, ty_args: &[Type]) -> PartialVMResult<Type> {
    if !ty_args.is_empty() {
        ty.subst(ty_args)
    } else {
        Ok(ty.to_type())
    }
}

pub fn instantiate_struct_type(
    struct_inst: &StructInstantiation,
    ty_args: &[Type],
) -> PartialVMResult<Type> {
    let type_params = struct_inst.type_params.to_ref();
    instantiate_datatype_common(&struct_inst.def_vtable_key, type_params, ty_args)
}

pub fn instantiate_enum_type(
    variant_inst: &VariantInstantiation,
    ty_args: &[Type],
) -> PartialVMResult<Type> {
    let enum_inst = variant_inst.enum_inst.to_ref();
    let type_params = enum_inst.type_params.to_ref();
    instantiate_datatype_common(&enum_inst.def_vtable_key, type_params, ty_args)
}

fn instantiate_datatype_common(
    datatype_key: &VirtualTableKey,
    type_params: &[ArenaType],
    ty_args: &[Type],
) -> PartialVMResult<Type> {
    // Before instantiating the type, count the # of nodes of all type arguments plus
    // existing type instantiation.
    // If that number is larger than MAX_TYPE_INSTANTIATION_NODES, refuse to construct this type.
    // This prevents constructing larger and larger types via datatype instantiation.
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

    Ok(Type::DatatypeInstantiation(Box::new((
        datatype_key.clone(),
        type_params
            .iter()
            .map(|ty| ty.subst(ty_args))
            .collect::<PartialVMResult<_>>()?,
    ))))
}
