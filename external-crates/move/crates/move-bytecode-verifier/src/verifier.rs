// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains the public APIs supported by the bytecode verifier.
use crate::{
    ability_cache::AbilityCache,
    ability_field_requirements,
    check_duplication::DuplicationChecker,
    code_unit_verifier::{self},
    constants,
    data_defs::RecursiveDataDefChecker,
    friends,
    instantiation_loops::InstantiationLoopChecker,
    instruction_consistency::InstructionConsistency,
    limits::LimitsVerifier,
    script_signature::{self, no_additional_script_signature_checks},
    signature::SignatureChecker,
};
use move_binary_format::{
    check_bounds::BoundsChecker,
    errors::{Location, VMResult},
    file_format::CompiledModule,
    file_format_common::VERSION_6,
};
use move_bytecode_verifier_meter::{dummy::DummyMeter, Meter};
use move_vm_config::verifier::VerifierConfig;
use std::time::Instant;

/// Helper for a "canonical" verification of a module.
///
/// Clients that rely on verification should call the proper passes
/// internally rather than using this function.
///
/// This function is intended to provide a verification path for clients
/// that do not require full control over verification. It is advised to
/// call this umbrella function instead of each individual checkers to
/// minimize the code locations that need to be updated should a new checker
/// is introduced.
pub fn verify_module_unmetered(module: &CompiledModule) -> VMResult<()> {
    verify_module_with_config_unmetered(&VerifierConfig::default(), module)
}

pub fn verify_module_with_config_for_test(
    name: &str,
    config: &VerifierConfig,
    module: &CompiledModule,
    meter: &mut (impl Meter + ?Sized),
) -> VMResult<()> {
    const MAX_MODULE_SIZE: usize = 65355;
    let mut bytes = vec![];
    let version = if config.bytecode_version > VERSION_6 {
        module.version
    } else {
        VERSION_6
    };
    module.serialize_with_version(version, &mut bytes).unwrap();
    let now = Instant::now();
    let result = verify_module_with_config_metered(config, module, meter);
    eprintln!(
        "--> {}: verification time: {:.3}ms, result: {}, size: {}kb",
        name,
        (now.elapsed().as_micros() as f64) / 1000.0,
        if let Err(e) = &result {
            format!("{:?}", e.major_status())
        } else {
            "Ok".to_string()
        },
        bytes.len() / 1000
    );
    // Also check whether the module actually fits into our payload size
    assert!(
        bytes.len() <= MAX_MODULE_SIZE,
        "test module exceeds size limit {} (given size {})",
        MAX_MODULE_SIZE,
        bytes.len()
    );
    result
}

pub fn verify_module_with_config_metered(
    config: &VerifierConfig,
    module: &CompiledModule,
    meter: &mut (impl Meter + ?Sized),
) -> VMResult<()> {
    let ability_cache = &mut AbilityCache::new(module);
    BoundsChecker::verify_module(module).map_err(|e| {
        // We can't point the error at the module, because if bounds-checking
        // failed, we cannot safely index into module's handle to itself.
        e.finish(Location::Undefined)
    })?;
    LimitsVerifier::verify_module(config, module)?;
    DuplicationChecker::verify_module(module)?;
    SignatureChecker::verify_module(module, ability_cache, meter)?;
    InstructionConsistency::verify_module(module)?;
    constants::verify_module(module)?;
    friends::verify_module(module)?;
    ability_field_requirements::verify_module(module, ability_cache, meter)?;
    RecursiveDataDefChecker::verify_module(module)?;
    InstantiationLoopChecker::verify_module(module)?;
    code_unit_verifier::verify_module(config, module, ability_cache, meter)?;

    script_signature::verify_module(module, no_additional_script_signature_checks)
}

pub fn verify_module_with_config_unmetered(
    config: &VerifierConfig,
    module: &CompiledModule,
) -> VMResult<()> {
    verify_module_with_config_metered(config, module, &mut DummyMeter)
}
