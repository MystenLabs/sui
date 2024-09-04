// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use invalid_mutations::{
    bounds::{
        ApplyCodeUnitBoundsContext, ApplyOutOfBoundsContext, CodeUnitBoundsMutation,
        OutOfBoundsMutation,
    },
    signature::{FieldRefMutation, SignatureRefMutation},
};
use move_binary_format::{
    check_bounds::BoundsChecker, file_format::CompiledModule,
    proptest_types::CompiledModuleStrategyGen,
};
use move_bytecode_verifier::{
    ability_cache::AbilityCache, ability_field_requirements, constants,
    instantiation_loops::InstantiationLoopChecker, DuplicationChecker, InstructionConsistency,
    RecursiveDataDefChecker, SignatureChecker,
};
use move_bytecode_verifier_meter::dummy::DummyMeter;
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, vm_status::StatusCode,
};
use proptest::{collection::vec, prelude::*, sample::Index as PropIndex};

proptest! {
    // Generating arbitrary compiled modules is really slow, possibly because of
    // https://github.com/AltSysrq/proptest/issues/143.
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn valid_ability_transitivity(module in CompiledModule::valid_strategy(20)) {
        let module = &module;
        let ability_cache = &mut AbilityCache::new(module);
        prop_assert!(ability_field_requirements::verify_module(module, ability_cache, &mut DummyMeter).is_ok());
    }

    #[test]
    fn valid_bounds(_module in CompiledModule::valid_strategy(20)) {
        // valid_strategy will panic if there are any bounds check issues.
    }

    #[test]
    fn invalid_out_of_bounds(
        module in CompiledModule::valid_strategy(20),
        oob_mutations in vec(OutOfBoundsMutation::strategy(), 0..40),
    ) {
        let (module, expected_violations) = {
            let oob_context = ApplyOutOfBoundsContext::new(module, oob_mutations);
            oob_context.apply()
        };

        let actual_violations = BoundsChecker::verify_module(&module);
        prop_assert_eq!(expected_violations.is_empty(), actual_violations.is_ok());
    }

    #[test]
    fn code_unit_out_of_bounds(
        mut module in CompiledModule::valid_strategy(20),
        mutations in vec(CodeUnitBoundsMutation::strategy(), 0..40),
    ) {
        let expected_violations = {
            let context = ApplyCodeUnitBoundsContext::new(&mut module, mutations);
            context.apply()
        };

        let actual_violations = BoundsChecker::verify_module(&module);
        prop_assert_eq!(expected_violations.is_empty(), actual_violations.is_ok());
    }

    #[test]
    fn no_module_handles(
        identifiers in vec(any::<Identifier>(), 0..20),
        address_identifiers in vec(any::<AccountAddress>(), 0..20),
    ) {
        // If there are no module handles, the only other things that can be stored are intrinsic
        // data.
        let module = CompiledModule {
            identifiers,
            address_identifiers,
            ..Default::default()
        };

        prop_assert_eq!(
            BoundsChecker::verify_module(&module).map_err(|e| e.major_status()),
            Err(StatusCode::NO_MODULE_HANDLES)
        );
    }

    /// Make sure that garbage inputs don't crash the bounds checker.
    #[test]
    fn garbage_inputs(module in any_with::<CompiledModule>(16)) {
        let _ = BoundsChecker::verify_module(&module);
    }

    #[test]
    fn valid_generated_constants(module in CompiledModule::valid_strategy(20)) {
        prop_assert!(constants::verify_module(&module).is_ok());
    }

    #[test]
    fn valid_duplication(module in CompiledModule::valid_strategy(20)) {
        prop_assert!(DuplicationChecker::verify_module(&module).is_ok());
    }

    #[test]
    fn check_verifier_passes(module in CompiledModule::valid_strategy(20)) {
        let module = &module;
        let ability_cache = &mut AbilityCache::new(module);
        DuplicationChecker::verify_module(module).expect("DuplicationChecker failure");
        SignatureChecker::verify_module(module, ability_cache, &mut DummyMeter).expect("SignatureChecker failure");
        InstructionConsistency::verify_module(module).expect("InstructionConsistency failure");
        constants::verify_module(module).expect("constants failure");
        ability_field_requirements::verify_module(module, ability_cache, &mut DummyMeter).expect("ability_field_requirements failure");
        RecursiveDataDefChecker::verify_module(module).expect("RecursiveDataDefChecker failure");
        InstantiationLoopChecker::verify_module(module).expect("InstantiationLoopChecker failure");
    }

    #[test]
    fn valid_signatures(module in CompiledModule::valid_strategy(20)) {
        let module = &module;
        let ability_cache = &mut AbilityCache::new(module);
        prop_assert!(SignatureChecker::verify_module(module, ability_cache, &mut DummyMeter).is_ok())
    }

    #[test]
    fn double_refs(
        mut module in CompiledModule::valid_strategy(20),
        mutations in vec((any::<PropIndex>(), any::<PropIndex>()), 0..20),
    ) {
        let context = SignatureRefMutation::new(&mut module, mutations);
        let expected_violations = context.apply();

        let module = &module;
        let ability_cache = &mut AbilityCache::new(module);
        let result = SignatureChecker::verify_module(module, ability_cache, &mut DummyMeter);

        prop_assert_eq!(expected_violations, result.is_err());
    }

    #[test]
    fn field_def_references(
        mut module in CompiledModule::valid_strategy(20),
        mutations in vec((any::<PropIndex>(), any::<PropIndex>()), 0..40),
    ) {
        let context = FieldRefMutation::new(&mut module, mutations);
        let expected_violations = context.apply();

        let module = &module;
        let ability_cache = &mut AbilityCache::new(module);
        let result = SignatureChecker::verify_module(module, ability_cache, &mut DummyMeter);

        prop_assert_eq!(expected_violations, result.is_err());
    }

    #[test]
    fn valid_recursive_struct_defs(module in CompiledModule::valid_strategy(20)) {
        prop_assert!(RecursiveDataDefChecker::verify_module(&module).is_ok());
    }
}

/// Ensure that valid modules that don't have any members (e.g. function args, struct fields) pass
/// bounds checks.
///
/// There are some potentially tricky edge cases around ranges that are captured here.
#[test]
fn valid_bounds_no_members() {
    let mut gen = CompiledModuleStrategyGen::new(20);
    gen.zeros_all();
    proptest!(|(_module in gen.generate())| {
        // gen.generate() will panic if there are any bounds check issues.
    });
}
