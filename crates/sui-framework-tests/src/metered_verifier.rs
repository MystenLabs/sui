// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_bytecode_verifier::meter::Scope;
use prometheus::Registry;
use std::{path::PathBuf, sync::Arc, time::Instant};
use sui_adapter::adapter::{default_verifier_config, run_metered_move_bytecode_verifier};
use sui_framework::BuiltInFramework;
use sui_move_build::{BuildConfig, SuiPackageHooks};
use sui_protocol_config::ProtocolConfig;
use sui_types::{error::SuiError, metrics::BytecodeVerifierMetrics};
use sui_verifier::meter::SuiVerifierMeter;

#[test]
#[cfg_attr(msim, ignore)]
fn test_metered_move_bytecode_verifier() {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sui-framework/packages/sui-framework");
    let compiled_package = BuildConfig::new_for_testing().build(path).unwrap();
    let compiled_modules: Vec<_> = compiled_package.get_modules().cloned().collect();

    let mut metered_verifier_config = default_verifier_config(
        &ProtocolConfig::get_for_max_version_UNSAFE(),
        true, /* enable metering */
    );
    let registry = &Registry::new();
    let bytecode_verifier_metrics = Arc::new(BytecodeVerifierMetrics::new(registry));
    let mut meter = SuiVerifierMeter::new(&metered_verifier_config);
    let timer_start = Instant::now();
    // Default case should pass
    let r = run_metered_move_bytecode_verifier(
        &compiled_modules,
        &metered_verifier_config,
        &mut meter,
        &bytecode_verifier_metrics,
    );
    let elapsed = timer_start.elapsed().as_micros() as f64 / (1000.0 * 1000.0);
    assert!(r.is_ok());

    // Ensure metrics worked as expected

    // The number of module success samples must equal the number of modules
    assert_eq!(
        compiled_modules.len() as u64,
        bytecode_verifier_metrics
            .verifier_runtime_per_module_success_latency
            .get_sample_count(),
    );

    // Others must be zero
    assert_eq!(
        0,
        bytecode_verifier_metrics
            .verifier_runtime_per_module_timeout_latency
            .get_sample_count(),
    );

    assert_eq!(
        0,
        bytecode_verifier_metrics
            .verifier_runtime_per_ptb_success_latency
            .get_sample_count(),
    );

    assert_eq!(
        0,
        bytecode_verifier_metrics
            .verifier_runtime_per_ptb_timeout_latency
            .get_sample_count(),
    );

    // Each success timer must be non zero and less than our elapsed time
    let module_success_latency = bytecode_verifier_metrics
        .verifier_runtime_per_module_success_latency
        .get_sample_sum();
    assert!(0.0 <= module_success_latency && module_success_latency < elapsed);

    // No failures expected in counter
    assert_eq!(
        0,
        bytecode_verifier_metrics
            .verifier_timeout_metrics
            .with_label_values(&[
                BytecodeVerifierMetrics::MOVE_VERIFIER_TAG,
                BytecodeVerifierMetrics::TIMEOUT_TAG,
            ])
            .get(),
    );

    assert_eq!(
        0,
        bytecode_verifier_metrics
            .verifier_timeout_metrics
            .with_label_values(&[
                BytecodeVerifierMetrics::SUI_VERIFIER_TAG,
                BytecodeVerifierMetrics::TIMEOUT_TAG,
            ])
            .get(),
    );

    // Counter must equal number of modules
    assert_eq!(
        compiled_modules.len() as u64,
        bytecode_verifier_metrics
            .verifier_timeout_metrics
            .with_label_values(&[
                BytecodeVerifierMetrics::OVERALL_TAG,
                BytecodeVerifierMetrics::SUCCESS_TAG,
            ])
            .get(),
    );

    // Use low limits. Should fail
    metered_verifier_config.max_back_edges_per_function = Some(100);
    metered_verifier_config.max_back_edges_per_module = Some(1_000);
    metered_verifier_config.max_per_mod_meter_units = Some(10_000);
    metered_verifier_config.max_per_fun_meter_units = Some(10_000);

    let mut meter = SuiVerifierMeter::new(&metered_verifier_config);
    let timer_start = Instant::now();
    let r = run_metered_move_bytecode_verifier(
        &compiled_modules,
        &metered_verifier_config,
        &mut meter,
        &bytecode_verifier_metrics,
    );
    let elapsed = timer_start.elapsed().as_micros() as f64 / (1000.0 * 1000.0);

    assert!(matches!(
        r.unwrap_err(),
        SuiError::ModuleVerificationFailure { .. }
    ));

    // Some new modules might have passed
    let module_success_samples = bytecode_verifier_metrics
        .verifier_runtime_per_module_success_latency
        .get_sample_count();
    let module_timeout_samples = bytecode_verifier_metrics
        .verifier_runtime_per_module_timeout_latency
        .get_sample_count();
    assert!(module_success_samples >= compiled_modules.len() as u64);
    assert!(module_success_samples < 2 * compiled_modules.len() as u64);
    assert!(module_timeout_samples > 0);

    // Others must be zero
    assert_eq!(
        0,
        bytecode_verifier_metrics
            .verifier_runtime_per_ptb_success_latency
            .get_sample_count(),
    );
    assert_eq!(
        0,
        bytecode_verifier_metrics
            .verifier_runtime_per_ptb_timeout_latency
            .get_sample_count(),
    );

    // Each success timer must be non zero and less than our elapsed time
    let module_timeout_latency = bytecode_verifier_metrics
        .verifier_runtime_per_module_timeout_latency
        .get_sample_sum();
    assert!(0.0 <= module_timeout_latency && module_timeout_latency < elapsed);

    // One failure
    assert_eq!(
        1,
        bytecode_verifier_metrics
            .verifier_timeout_metrics
            .with_label_values(&[
                BytecodeVerifierMetrics::MOVE_VERIFIER_TAG,
                BytecodeVerifierMetrics::TIMEOUT_TAG,
            ])
            .get(),
    );
    // Sui verifier did not fail
    assert_eq!(
        0,
        bytecode_verifier_metrics
            .verifier_timeout_metrics
            .with_label_values(&[
                BytecodeVerifierMetrics::SUI_VERIFIER_TAG,
                BytecodeVerifierMetrics::TIMEOUT_TAG,
            ])
            .get(),
    );

    // This should be slightly higher as some modules passed
    assert!(
        bytecode_verifier_metrics
            .verifier_timeout_metrics
            .with_label_values(&[
                BytecodeVerifierMetrics::OVERALL_TAG,
                BytecodeVerifierMetrics::SUCCESS_TAG,
            ])
            .get()
            > compiled_modules.len() as u64
    );

    // Check shared meter logic works across all publish in PT
    let mut packages = vec![];
    let with_unpublished_deps = false;
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sui_programmability/examples/basics");
    let package = BuildConfig::new_for_testing().build(path).unwrap();
    packages.push(package.get_dependency_sorted_modules(with_unpublished_deps));
    packages.push(package.get_dependency_sorted_modules(with_unpublished_deps));

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sui_programmability/examples/fungible_tokens");
    let package = BuildConfig::new_for_testing().build(path).unwrap();
    packages.push(package.get_dependency_sorted_modules(with_unpublished_deps));

    let is_metered = true;
    let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    let metered_verifier_config = default_verifier_config(&protocol_config, is_metered);

    // Check if the same meter is indeed used multiple invocations of the verifier
    let mut meter = SuiVerifierMeter::new(&metered_verifier_config);
    for modules in &packages {
        let prev_meter = meter.get_usage(Scope::Module) + meter.get_usage(Scope::Function);

        run_metered_move_bytecode_verifier(
            modules,
            &metered_verifier_config,
            &mut meter,
            &bytecode_verifier_metrics,
        )
        .expect("Verification should not timeout");

        let curr_meter = meter.get_usage(Scope::Module) + meter.get_usage(Scope::Function);
        assert!(curr_meter > prev_meter);
    }
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_meter_system_packages() {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));

    let is_metered = true;
    let metered_verifier_config =
        default_verifier_config(&ProtocolConfig::get_for_max_version_UNSAFE(), is_metered);
    let registry = &Registry::new();
    let bytecode_verifier_metrics = Arc::new(BytecodeVerifierMetrics::new(registry));
    let mut meter = SuiVerifierMeter::new(&metered_verifier_config);
    for system_package in BuiltInFramework::iter_system_packages() {
        run_metered_move_bytecode_verifier(
            &system_package.modules(),
            &metered_verifier_config,
            &mut meter,
            &bytecode_verifier_metrics,
        )
        .unwrap_or_else(|_| {
            panic!(
                "Verification of all system packages should succeed, but failed on {}",
                system_package.id(),
            )
        });
    }

    // Ensure metrics worked as expected
    // No failures expected in counter
    assert_eq!(
        0,
        bytecode_verifier_metrics
            .verifier_timeout_metrics
            .with_label_values(&[
                BytecodeVerifierMetrics::MOVE_VERIFIER_TAG,
                BytecodeVerifierMetrics::TIMEOUT_TAG,
            ])
            .get(),
    );
    assert_eq!(
        0,
        bytecode_verifier_metrics
            .verifier_timeout_metrics
            .with_label_values(&[
                BytecodeVerifierMetrics::SUI_VERIFIER_TAG,
                BytecodeVerifierMetrics::TIMEOUT_TAG,
            ])
            .get(),
    );

    // Counter must equal number of modules
    assert_eq!(
        BuiltInFramework::iter_system_packages()
            .map(|p| p.modules().len() as u64)
            .sum::<u64>(),
        bytecode_verifier_metrics
            .verifier_timeout_metrics
            .with_label_values(&[
                BytecodeVerifierMetrics::OVERALL_TAG,
                BytecodeVerifierMetrics::SUCCESS_TAG,
            ])
            .get()
    );
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_build_and_verify_programmability_examples() {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));

    let is_metered = true;
    let metered_verifier_config =
        default_verifier_config(&ProtocolConfig::get_for_max_version_UNSAFE(), is_metered);
    let registry = &Registry::new();
    let bytecode_verifier_metrics = Arc::new(BytecodeVerifierMetrics::new(registry));
    let examples =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sui_programmability/examples");

    for example in std::fs::read_dir(&examples).unwrap() {
        let Ok(example) = example else { continue };
        let path = example.path();

        if !path.is_dir() {
            continue;
        }

        let manifest = path.join("Move.toml");
        if !manifest.exists() {
            continue;
        };

        let modules = BuildConfig::new_for_testing()
            .build(path)
            .unwrap()
            .into_modules();

        let mut meter = SuiVerifierMeter::new(&metered_verifier_config);
        run_metered_move_bytecode_verifier(
            &modules,
            &metered_verifier_config,
            &mut meter,
            &bytecode_verifier_metrics,
        )
        .unwrap_or_else(|_| {
            panic!(
                "Verification of example: '{:?}' failed",
                example.file_name(),
            )
        });
    }
}
