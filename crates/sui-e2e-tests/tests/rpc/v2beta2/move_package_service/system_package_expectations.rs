// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::proto::sui::rpc::v2beta2::{
    datatype_descriptor::DatatypeKind, function_descriptor::Visibility, open_signature,
    open_signature_body, Ability, DatatypeDescriptor, FunctionDescriptor, Module, Package,
};

pub fn validate_system_package(package: &Package) {
    assert_eq!(
        package.storage_id,
        Some("0x0000000000000000000000000000000000000000000000000000000000000003".to_string())
    );
    assert_eq!(
        package.original_id,
        Some("0x0000000000000000000000000000000000000000000000000000000000000003".to_string())
    );
    assert_eq!(package.version, Some(1));

    // Validate exact module count
    let modules = &package.modules;
    assert_eq!(11, modules.len());

    // Extract and validate module names
    let module_names: Vec<String> = modules.iter().filter_map(|m| m.name.clone()).collect();

    let expected_modules = vec![
        "genesis".to_string(),
        "stake_subsidy".to_string(),
        "staking_pool".to_string(),
        "storage_fund".to_string(),
        "sui_system".to_string(),
        "sui_system_state_inner".to_string(),
        "validator".to_string(),
        "validator_cap".to_string(),
        "validator_set".to_string(),
        "validator_wrapper".to_string(),
        "voting_power".to_string(),
    ];

    assert_eq!(module_names, expected_modules);

    // Deeply validate validator_cap module
    let validator_cap_module = modules
        .iter()
        .find(|m| {
            m.name
                .as_ref()
                .map(|n| n == "validator_cap")
                .unwrap_or(false)
        })
        .expect("validator_cap module should exist in system package");
    validate_validator_cap_module(validator_cap_module);
}

pub fn validate_validator_cap_module(module: &Module) {
    assert_eq!(module.name.as_ref().unwrap(), "validator_cap");

    // Validate datatypes
    assert_eq!(module.datatypes.len(), 2);
    let datatype_names: Vec<_> = module
        .datatypes
        .iter()
        .filter_map(|dt| dt.name.clone())
        .collect();

    assert!(datatype_names.contains(&"UnverifiedValidatorOperationCap".to_string()));
    assert!(datatype_names.contains(&"ValidatorOperationCap".to_string()));

    // Find and validate ValidatorOperationCap datatype
    let validator_op_cap = module
        .datatypes
        .iter()
        .find(|dt| {
            dt.name
                .as_ref()
                .map(|n| n == "ValidatorOperationCap")
                .unwrap_or(false)
        })
        .expect("ValidatorOperationCap should exist");
    validate_validator_operation_cap_datatype(validator_op_cap);

    // Validate functions
    assert_eq!(module.functions.len(), 4);
    let function_names: Vec<_> = module
        .functions
        .iter()
        .filter_map(|f| f.name.clone())
        .collect();

    let expected_functions = vec![
        "into_verified".to_string(),
        "new_unverified_validator_operation_cap_and_transfer".to_string(),
        "unverified_operation_cap_address".to_string(),
        "verified_operation_cap_address".to_string(),
    ];

    assert_eq!(function_names, expected_functions);

    // Find and validate new_unverified_validator_operation_cap_and_transfer function
    let new_unverified_fn = module
        .functions
        .iter()
        .find(|f| {
            f.name
                .as_ref()
                .map(|n| n == "new_unverified_validator_operation_cap_and_transfer")
                .unwrap_or(false)
        })
        .expect("new_unverified_validator_operation_cap_and_transfer should exist");
    validate_new_unverified_validator_operation_cap_and_transfer_function(new_unverified_fn);
}

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

    // Check abilities - only has Drop
    assert_eq!(datatype.abilities.len(), 1);
    assert!(datatype.abilities.contains(&(Ability::Drop as i32)));

    // Check no type parameters
    assert_eq!(datatype.type_parameters.len(), 0);

    // Check fields
    assert_eq!(datatype.fields.len(), 1);

    let field = &datatype.fields[0];
    assert_eq!(field.name.as_ref().unwrap(), "authorizer_validator_address");
    assert_eq!(field.position, Some(0));

    // Validate field type
    let field_type = field.r#type.as_ref().unwrap();
    assert_eq!(
        field_type.r#type,
        Some(open_signature_body::Type::Address as i32)
    );
    assert_eq!(field_type.type_name, None);
    assert_eq!(field_type.type_parameter_instantiation.len(), 0);
    assert_eq!(field_type.type_parameter, None);

    // Check variants - should be empty for struct
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

    // Check type parameters
    assert_eq!(function.type_parameters.len(), 0);

    // Check parameters
    assert_eq!(function.parameters.len(), 2);

    // Parameter 0: address
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

    // Parameter 1: &mut TxContext
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
        Some("0x0000000000000000000000000000000000000000000000000000000000000002::tx_context::TxContext".to_string())
    );
    assert_eq!(param1_body.type_parameter_instantiation.len(), 0);
    assert_eq!(param1_body.type_parameter, None);

    // Check returns
    assert_eq!(function.returns.len(), 1);

    // Return: object::ID
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
