// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared validators for the system Move package (0x3) used by
//! `get_datatype` and `get_function` ports. Mirrors
//! `sui-e2e-tests/tests/rpc/v2/move_package_service/system_package_expectations.rs`,
//! trimmed to just the entries those tests actually call into.

use sui_rpc::proto::sui::rpc::v2::Ability;
use sui_rpc::proto::sui::rpc::v2::DatatypeDescriptor;
use sui_rpc::proto::sui::rpc::v2::FunctionDescriptor;
use sui_rpc::proto::sui::rpc::v2::datatype_descriptor::DatatypeKind;
use sui_rpc::proto::sui::rpc::v2::function_descriptor::Visibility;
use sui_rpc::proto::sui::rpc::v2::open_signature;
use sui_rpc::proto::sui::rpc::v2::open_signature_body;

pub fn validate_validator_operation_cap_datatype(datatype: &DatatypeDescriptor) {
    assert_eq!(
        datatype.type_name.as_ref().unwrap(),
        "0x0000000000000000000000000000000000000000000000000000000000000003::validator_cap::ValidatorOperationCap"
    );
    assert_eq!(datatype.name.as_ref().unwrap(), "ValidatorOperationCap");
    assert_eq!(datatype.module.as_ref().unwrap(), "validator_cap");
    assert_eq!(
        datatype.defining_id.as_ref().unwrap(),
        "0x0000000000000000000000000000000000000000000000000000000000000003"
    );
    assert_eq!(datatype.kind, Some(DatatypeKind::Struct as i32));

    assert_eq!(datatype.abilities.len(), 1);
    assert!(datatype.abilities.contains(&(Ability::Drop as i32)));

    assert_eq!(datatype.type_parameters.len(), 0);

    assert_eq!(datatype.fields.len(), 1);
    let field = &datatype.fields[0];
    assert_eq!(field.name.as_ref().unwrap(), "authorizer_validator_address");
    assert_eq!(field.position, Some(0));

    let field_type = field.r#type.as_ref().unwrap();
    assert_eq!(
        field_type.r#type,
        Some(open_signature_body::Type::Address as i32)
    );
    assert_eq!(field_type.type_name, None);
    assert_eq!(field_type.type_parameter_instantiation.len(), 0);
    assert_eq!(field_type.type_parameter, None);

    assert_eq!(datatype.variants.len(), 0);
}

pub fn validate_new_unverified_validator_operation_cap_and_transfer_function(
    function: &FunctionDescriptor,
) {
    assert_eq!(
        function.name.as_ref().unwrap(),
        "new_unverified_validator_operation_cap_and_transfer"
    );
    assert_eq!(function.visibility, Some(Visibility::Friend as i32));
    assert_eq!(function.is_entry, Some(false));

    assert_eq!(function.type_parameters.len(), 0);
    assert_eq!(function.parameters.len(), 2);

    // Parameter 0: address.
    let param0 = &function.parameters[0];
    assert_eq!(param0.reference, None);
    let param0_body = param0.body.as_ref().unwrap();
    assert_eq!(
        param0_body.r#type,
        Some(open_signature_body::Type::Address as i32)
    );
    assert_eq!(param0_body.type_name, None);
    assert_eq!(param0_body.type_parameter_instantiation.len(), 0);
    assert_eq!(param0_body.type_parameter, None);

    // Parameter 1: &mut TxContext.
    let param1 = &function.parameters[1];
    assert_eq!(
        param1.reference,
        Some(open_signature::Reference::Mutable as i32)
    );
    let param1_body = param1.body.as_ref().unwrap();
    assert_eq!(
        param1_body.r#type,
        Some(open_signature_body::Type::Datatype as i32)
    );
    assert_eq!(
        param1_body.type_name,
        Some(
            "0x0000000000000000000000000000000000000000000000000000000000000002::tx_context::TxContext"
                .to_string()
        )
    );
    assert_eq!(param1_body.type_parameter_instantiation.len(), 0);
    assert_eq!(param1_body.type_parameter, None);

    assert_eq!(function.returns.len(), 1);
    let return0 = &function.returns[0];
    assert_eq!(return0.reference, None);
    let return0_body = return0.body.as_ref().unwrap();
    assert_eq!(
        return0_body.r#type,
        Some(open_signature_body::Type::Datatype as i32)
    );
    assert_eq!(
        return0_body.type_name,
        Some(
            "0x0000000000000000000000000000000000000000000000000000000000000002::object::ID"
                .to_string()
        )
    );
    assert_eq!(return0_body.type_parameter_instantiation.len(), 0);
    assert_eq!(return0_body.type_parameter, None);
}
