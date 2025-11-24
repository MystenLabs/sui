// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module implements the checker for verifying correctness of function bodies.
//! The overall verification is split between stack_usage_verifier.rs and
//! abstract_interpreter.rs. CodeUnitVerifier simply orchestrates calls into these two files.
use crate::{
    ability_cache::AbilityCache, absint::FunctionContext, acquires_list_verifier::AcquiresVerifier,
    control_flow, locals_safety, reference_safety, regex_reference_safety,
    stack_usage_verifier::StackUsageVerifier, type_safety,
};
use move_abstract_interpreter::control_flow_graph::ControlFlowGraph;
use move_binary_format::{
    IndexKind,
    errors::{Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{
        CompiledModule, FunctionDefinition, FunctionDefinitionIndex, IdentifierIndex, TableIndex,
    },
};
use move_bytecode_verifier_meter::{Meter, Scope, bound::BoundMeter};
use move_core_types::vm_status::StatusCode;
use move_vm_config::verifier::VerifierConfig;
use std::collections::HashMap;

pub struct CodeUnitVerifier<'env, 'a> {
    module: &'env CompiledModule,
    function_context: FunctionContext<'env>,
    name_def_map: &'a HashMap<IdentifierIndex, FunctionDefinitionIndex>,
}

pub fn verify_module<'env>(
    verifier_config: &'env VerifierConfig,
    module: &'env CompiledModule,
    ability_cache: &mut AbilityCache<'env>,
    meter: &mut (impl Meter + ?Sized),
) -> VMResult<()> {
    let mut regex_reference_safety_meter =
        if let Some(limit) = verifier_config.sanity_check_with_regex_reference_safety {
            let module_name = module.identifier_at(module.self_handle().name).as_str();
            let mut m = BoundMeter::new(move_vm_config::verifier::MeterConfig {
                max_per_fun_meter_units: Some(limit),
                max_per_mod_meter_units: Some(limit),
                max_per_pkg_meter_units: Some(limit),
            });
            m.enter_scope(module_name, Scope::Module);
            m
        } else {
            // unused
            BoundMeter::new(move_vm_config::verifier::MeterConfig {
                max_per_fun_meter_units: None,
                max_per_mod_meter_units: None,
                max_per_pkg_meter_units: None,
            })
        };
    verify_module_impl(
        verifier_config,
        module,
        ability_cache,
        meter,
        &mut regex_reference_safety_meter,
    )
    .map_err(|e| e.finish(Location::Module(module.self_id())))
}

fn verify_module_impl<'env>(
    verifier_config: &'env VerifierConfig,
    module: &'env CompiledModule,
    ability_cache: &mut AbilityCache<'env>,
    meter: &mut (impl Meter + ?Sized),
    regex_reference_safety_meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    let mut name_def_map = HashMap::new();
    for (idx, func_def) in module.function_defs().iter().enumerate() {
        let fh = module.function_handle_at(func_def.function);
        name_def_map.insert(fh.name, FunctionDefinitionIndex(idx as u16));
    }
    let mut total_back_edges = 0;
    for (idx, function_definition) in module.function_defs().iter().enumerate() {
        let index = FunctionDefinitionIndex(idx as TableIndex);
        let num_back_edges = verify_function(
            verifier_config,
            index,
            function_definition,
            module,
            ability_cache,
            &name_def_map,
            meter,
            regex_reference_safety_meter,
        )
        .map_err(|err| err.at_index(IndexKind::FunctionDefinition, index.0))?;
        total_back_edges += num_back_edges;
    }
    if let Some(limit) = verifier_config.max_back_edges_per_module
        && total_back_edges > limit
    {
        return Err(PartialVMError::new(StatusCode::TOO_MANY_BACK_EDGES));
    }
    Ok(())
}

pub fn verify_function<'env>(
    verifier_config: &'env VerifierConfig,
    index: FunctionDefinitionIndex,
    function_definition: &'env FunctionDefinition,
    module: &'env CompiledModule,
    ability_cache: &mut AbilityCache<'env>,
    name_def_map: &HashMap<IdentifierIndex, FunctionDefinitionIndex>,
    meter: &mut (impl Meter + ?Sized),
    regex_reference_safety_meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<usize> {
    let function_name = module
        .identifier_at(module.function_handle_at(function_definition.function).name)
        .as_str();
    meter.enter_scope(function_name, Scope::Function);
    regex_reference_safety_meter.enter_scope(function_name, Scope::Function);
    // nothing to verify for native function
    let code = match &function_definition.code {
        Some(code) => code,
        None => return Ok(0),
    };

    // create `FunctionContext` and `BinaryIndexedView`
    let function_context = control_flow::verify_function(
        verifier_config,
        module,
        index,
        function_definition,
        code,
        meter,
    )?;

    if let Some(limit) = verifier_config.max_basic_blocks
        && function_context.cfg().blocks().count() > limit
    {
        return Err(PartialVMError::new(StatusCode::TOO_MANY_BASIC_BLOCKS).at_code_offset(index, 0));
    }

    let num_back_edges = function_context.cfg().num_back_edges();
    if let Some(limit) = verifier_config.max_back_edges_per_function
        && num_back_edges > limit
    {
        return Err(PartialVMError::new(StatusCode::TOO_MANY_BACK_EDGES).at_code_offset(index, 0));
    }

    // verify
    let code_unit_verifier = CodeUnitVerifier {
        module,
        function_context,
        name_def_map,
    };
    code_unit_verifier.verify_common(
        verifier_config,
        ability_cache,
        meter,
        regex_reference_safety_meter,
    )?;
    AcquiresVerifier::verify(verifier_config, module, index, function_definition, meter)?;

    meter.transfer(Scope::Function, Scope::Module, 1.0)?;

    Ok(num_back_edges)
}

impl<'env> CodeUnitVerifier<'env, '_> {
    fn verify_common(
        &self,
        verifier_config: &'env VerifierConfig,
        ability_cache: &mut AbilityCache<'env>,
        meter: &mut (impl Meter + ?Sized),
        regex_reference_safety_meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        StackUsageVerifier::verify(verifier_config, self.module, &self.function_context, meter)?;
        type_safety::verify(
            verifier_config,
            self.module,
            &self.function_context,
            ability_cache,
            meter,
        )?;
        locals_safety::verify(self.module, &self.function_context, ability_cache, meter)?;
        let reference_safety_res = reference_safety::verify(
            verifier_config,
            self.module,
            &self.function_context,
            self.name_def_map,
            meter,
        );
        if reference_safety_res.as_ref().is_err_and(|e| {
            e.major_status() == StatusCode::CONSTRAINT_NOT_SATISFIED
                || e.major_status() == StatusCode::PROGRAM_TOO_COMPLEX
        }) {
            // skip consistency check on timeout/complexity errors
            return reference_safety_res;
        }
        if verifier_config
            .sanity_check_with_regex_reference_safety
            .is_some()
        {
            let regex_res = regex_reference_safety::verify(
                verifier_config,
                self.module,
                &self.function_context,
                regex_reference_safety_meter,
            );
            if regex_res.as_ref().is_err_and(|e| {
                e.major_status() == StatusCode::CONSTRAINT_NOT_SATISFIED
                    || e.major_status() == StatusCode::PROGRAM_TOO_COMPLEX
            }) {
                // If the regex based checker fails due to complexity,
                // we reject it for being too complex and skip the consistency check.
                return Err(
                    PartialVMError::new(StatusCode::PROGRAM_TOO_COMPLEX).with_message(
                        regex_res
                            .unwrap_err()
                            .finish(Location::Undefined)
                            .message()
                            .cloned()
                            .unwrap_or_default(),
                    ),
                );
            }
            // The regular expression based reference safety check should be strictly more
            // permissive. So if it errors, the current one should also error.
            // As such, we assert: regex err ==> reference safety err
            // which is equivalent to: !regex_res.is_err() || reference_safety_res.is_err()
            // which is equivalent to: regex_res.is_ok() || reference_safety_res.is_err()
            let is_consistent = regex_res.is_ok() || reference_safety_res.is_err();
            if !is_consistent {
                return Err(
                    PartialVMError::new(StatusCode::REFERENCE_SAFETY_INCONSISTENT).with_message(
                        "regex reference safety should be strictly more permissive \
                         than the current"
                            .to_string(),
                    ),
                );
            }
        }
        reference_safety_res
    }
}
