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
    shared::TypeLimits,
};

use move_binary_format::{errors::PartialVMResult, partial_vm_error};

pub fn instantiate_generic_function(
    limits: &TypeLimits,
    fun_inst: &FunctionInstantiation,
    type_params: &[Type],
) -> PartialVMResult<Vec<Type>> {
    let instantiation: Vec<_> = fun_inst
        .instantiation
        .to_ref()
        .iter()
        .map(|ty| ty.subst_with_limits(limits, type_params))
        .collect::<PartialVMResult<_>>()?;

    // Check if the function instantiation over all generics exceeds the type node limit.
    let mut sum_nodes = 1u64;
    for ty in type_params.iter().chain(instantiation.iter()) {
        sum_nodes = sum_nodes.saturating_add(ty.count_type_nodes()?);
        if sum_nodes > limits.max_type_nodes() {
            return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
        }
    }
    Ok(instantiation)
}

pub fn instantiate_single_type(
    limits: &TypeLimits,
    ty: &ArenaType,
    ty_args: &[Type],
) -> PartialVMResult<Type> {
    if !ty_args.is_empty() {
        ty.subst_with_limits(limits, ty_args)
    } else {
        ty.to_type_with_limits(limits)
    }
}

pub fn instantiate_struct_type(
    limits: &TypeLimits,
    struct_inst: &StructInstantiation,
    ty_args: &[Type],
) -> PartialVMResult<Type> {
    let type_params = struct_inst.type_params.to_ref();
    instantiate_datatype_common(limits, &struct_inst.def_vtable_key, type_params, ty_args)
}

pub fn instantiate_enum_type(
    limits: &TypeLimits,
    variant_inst: &VariantInstantiation,
    ty_args: &[Type],
) -> PartialVMResult<Type> {
    let enum_inst = variant_inst.enum_inst.to_ref();
    let type_params = enum_inst.type_params.to_ref();
    instantiate_datatype_common(limits, &enum_inst.def_vtable_key, type_params, ty_args)
}

fn instantiate_datatype_common(
    limits: &TypeLimits,
    datatype_key: &VirtualTableKey,
    type_params: &[ArenaType],
    ty_args: &[Type],
) -> PartialVMResult<Type> {
    // Before instantiating the type, count the # of nodes of all type arguments plus
    // existing type instantiation.
    // This prevents constructing larger and larger types via datatype instantiation.
    let mut sum_nodes = 1u64;
    for ty in type_params.iter() {
        sum_nodes = sum_nodes.saturating_add(ty.count_type_nodes()?);
        if sum_nodes > limits.max_type_nodes() {
            return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
        }
    }
    for ty in ty_args.iter() {
        sum_nodes = sum_nodes.saturating_add(ty.count_type_nodes()?);
        if sum_nodes > limits.max_type_nodes() {
            return Err(partial_vm_error!(VM_MAX_TYPE_NODES_REACHED));
        }
    }

    Ok(Type::DatatypeInstantiation(Box::new((
        datatype_key.clone(),
        type_params
            .iter()
            .map(|ty| ty.subst_with_limits(limits, ty_args))
            .collect::<PartialVMResult<_>>()?,
    ))))
}
