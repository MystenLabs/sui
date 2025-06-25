// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This module contains conversion functions from sui-package-resolver types to their protobuf representations.
// We use explicit conversion functions instead of From traits because:
// 1. The orphan rule prevents implementing From for external types
// 2. Many conversions require additional context (e.g., package_id, module_name) that From traits cannot provide

use crate::proto::rpc::v2alpha::{
    datatype_descriptor::DatatypeKind, function_descriptor::Visibility, open_signature::Reference,
    open_signature_body::Type as SignatureType, Ability, AbilitySet, DatatypeDescriptor,
    FieldDescriptor, FunctionDescriptor, Module, OpenSignature, OpenSignatureBody, TypeParameter,
    VariantDescriptor,
};
use crate::{Result, RpcError};
use move_binary_format::file_format::{
    Ability as MoveAbility, AbilitySet as MoveAbilitySet, DatatypeTyParameter,
    Visibility as MoveVisibility,
};
use sui_package_resolver::{DataDef, FunctionDef, MoveData, VariantDef};
use sui_types::base_types::ObjectID;

pub(crate) fn convert_error(e: sui_package_resolver::error::Error) -> RpcError {
    RpcError::new(tonic::Code::Internal, e.to_string())
}

pub(crate) fn convert_datatype(
    datatype_name: &str,
    data_def: &DataDef,
    package_id: &ObjectID,
    module_name: &str,
) -> DatatypeDescriptor {
    let type_parameters = data_def
        .type_params
        .iter()
        .map(convert_type_parameter)
        .collect();
    let abilities = convert_ability_set(data_def.abilities);

    let (kind, fields, variants) = match &data_def.data {
        MoveData::Struct(fields) => {
            let proto_fields = fields
                .iter()
                .enumerate()
                .map(|(pos, (name, sig))| FieldDescriptor {
                    name: Some(name.clone()),
                    position: Some(pos as u32),
                    r#type: Some(convert_open_signature_body(sig)),
                })
                .collect();
            (DatatypeKind::Struct, proto_fields, vec![])
        }
        MoveData::Enum(variants) => {
            let proto_variants = variants
                .iter()
                .enumerate()
                .map(|(pos, variant)| convert_variant(pos, variant))
                .collect();
            (DatatypeKind::Enum, vec![], proto_variants)
        }
    };

    DatatypeDescriptor {
        type_name: Some(format!(
            "{}::{}::{}",
            package_id.to_hex_literal(),
            module_name,
            datatype_name
        )),
        defining_id: Some(data_def.defining_id.to_hex_literal()),
        module: Some(module_name.to_string()),
        name: Some(datatype_name.to_string()),
        abilities,
        type_parameters,
        kind: Some(kind as i32),
        fields,
        variants,
    }
}

pub(crate) fn convert_module(
    module_name: &str,
    resolved_module: &sui_package_resolver::Module,
    package_id: &ObjectID,
) -> Result<Module> {
    let mut datatypes = Vec::new();
    for datatype_name in resolved_module.datatypes(None, None) {
        let data_def = resolved_module
            .data_def(datatype_name)
            .map_err(convert_error)?
            .unwrap_or_else(|| {
                panic!(
                    "datatype {} does not have a data_def. This shouldn't happen.",
                    datatype_name
                )
            });

        let descriptor = convert_datatype(datatype_name, &data_def, package_id, module_name);
        datatypes.push(descriptor);
    }

    let mut functions = Vec::new();
    for func_name in resolved_module.functions(None, None) {
        let func_def = resolved_module
            .function_def(func_name)
            .map_err(convert_error)?
            .unwrap_or_else(|| {
                panic!(
                    "function {} does not have a function_def. This shouldn't happen.",
                    func_name
                )
            });

        let descriptor = convert_function(func_name, &func_def);
        functions.push(descriptor);
    }

    Ok(Module {
        name: Some(module_name.to_string()),
        datatypes,
        functions,
    })
}

pub(crate) fn convert_function(function_name: &str, func_def: &FunctionDef) -> FunctionDescriptor {
    let visibility = match func_def.visibility {
        MoveVisibility::Private => Visibility::Private,
        MoveVisibility::Public => Visibility::Public,
        MoveVisibility::Friend => Visibility::Friend,
    };

    let type_parameters = func_def
        .type_params
        .iter()
        .map(|abilities| AbilitySet {
            abilities: convert_ability_set(*abilities),
        })
        .collect();

    let parameters = func_def
        .parameters
        .iter()
        .map(convert_open_signature)
        .collect();
    let returns = func_def
        .return_
        .iter()
        .map(convert_open_signature)
        .collect();

    FunctionDescriptor {
        name: Some(function_name.to_string()),
        visibility: Some(visibility as i32),
        is_entry: Some(func_def.is_entry),
        type_parameters,
        parameters,
        returns,
    }
}

fn convert_type_parameter(tp: &DatatypeTyParameter) -> TypeParameter {
    TypeParameter {
        constraints: convert_ability_set(tp.constraints),
        is_phantom: Some(tp.is_phantom),
    }
}

fn convert_ability_set(abilities: MoveAbilitySet) -> Vec<i32> {
    let mut result = Vec::new();

    if abilities.has_ability(MoveAbility::Copy) {
        result.push(Ability::Copy as i32);
    }
    if abilities.has_ability(MoveAbility::Drop) {
        result.push(Ability::Drop as i32);
    }
    if abilities.has_ability(MoveAbility::Store) {
        result.push(Ability::Store as i32);
    }
    if abilities.has_ability(MoveAbility::Key) {
        result.push(Ability::Key as i32);
    }

    result
}

fn convert_variant(position: usize, variant: &VariantDef) -> VariantDescriptor {
    let fields = variant
        .signatures
        .iter()
        .enumerate()
        .map(|(pos, (name, sig))| FieldDescriptor {
            name: Some(name.clone()),
            position: Some(pos as u32),
            r#type: Some(convert_open_signature_body(sig)),
        })
        .collect();

    VariantDescriptor {
        name: Some(variant.name.clone()),
        position: Some(position as u32),
        fields,
    }
}

fn convert_open_signature(sig: &sui_package_resolver::OpenSignature) -> OpenSignature {
    let reference = sig.ref_.map(|r| match r {
        sui_package_resolver::Reference::Immutable => Reference::Immutable as i32,
        sui_package_resolver::Reference::Mutable => Reference::Mutable as i32,
    });

    OpenSignature {
        reference,
        body: Some(convert_open_signature_body(&sig.body)),
    }
}

fn convert_open_signature_body(
    body: &sui_package_resolver::OpenSignatureBody,
) -> OpenSignatureBody {
    let (type_enum, type_name, type_params, type_param_idx) = match body {
        sui_package_resolver::OpenSignatureBody::Bool => (SignatureType::Bool, None, vec![], None),
        sui_package_resolver::OpenSignatureBody::U8 => (SignatureType::U8, None, vec![], None),
        sui_package_resolver::OpenSignatureBody::U16 => (SignatureType::U16, None, vec![], None),
        sui_package_resolver::OpenSignatureBody::U32 => (SignatureType::U32, None, vec![], None),
        sui_package_resolver::OpenSignatureBody::U64 => (SignatureType::U64, None, vec![], None),
        sui_package_resolver::OpenSignatureBody::U128 => (SignatureType::U128, None, vec![], None),
        sui_package_resolver::OpenSignatureBody::U256 => (SignatureType::U256, None, vec![], None),
        sui_package_resolver::OpenSignatureBody::Address => {
            (SignatureType::Address, None, vec![], None)
        }
        sui_package_resolver::OpenSignatureBody::Vector(inner) => {
            let inner_body = convert_open_signature_body(inner.as_ref());
            (SignatureType::Vector, None, vec![inner_body], None)
        }
        sui_package_resolver::OpenSignatureBody::Datatype(key, args) => {
            let type_name = format!(
                "{}::{}::{}",
                key.package.to_hex_literal(),
                key.module,
                key.name
            );

            let type_args = args.iter().map(convert_open_signature_body).collect();
            (SignatureType::Datatype, Some(type_name), type_args, None)
        }
        sui_package_resolver::OpenSignatureBody::TypeParameter(idx) => {
            (SignatureType::Parameter, None, vec![], Some(*idx as u32))
        }
    };

    OpenSignatureBody {
        r#type: Some(type_enum as i32),
        type_name,
        type_parameter_instantiation: type_params,
        type_parameter: type_param_idx,
    }
}
