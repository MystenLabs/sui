// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::build;
use clap::Parser;
use move_cli::base::{
    self,
    test::{self, UnitTestResult},
};
use move_package::BuildConfig;
use move_unit_test::{extensions::set_extension_hook, UnitTestingConfig};
use move_vm_runtime::native_extensions::NativeContextExtensions;
use once_cell::sync::Lazy;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use sui_core::authority::TemporaryStore;
use sui_cost_tables::bytecode_tables::initial_cost_schedule_for_unit_tests;
use sui_move_natives::{object_runtime::ObjectRuntime, NativesCostTable};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    digests::TransactionDigest, in_memory_storage::InMemoryStorage, metrics::LimitsMetrics,
    transaction::InputObjects,
};

// Move unit tests will halt after executing this many steps. This is a protection to avoid divergence
const MAX_UNIT_TEST_INSTRUCTIONS: u64 = 1_000_000;

#[derive(Parser)]
pub struct Test {
    #[clap(flatten)]
    pub test: test::Test,
}

impl Test {
    pub fn execute(
        &self,
        path: Option<PathBuf>,
        build_config: BuildConfig,
        unit_test_config: UnitTestingConfig,
    ) -> anyhow::Result<UnitTestResult> {
        // find manifest file directory from a given path or (if missing) from current dir
        let rerooted_path = base::reroot_path(path)?;
        // pre build for Sui-specific verifications
        let with_unpublished_deps = false;
        let legacy_digest = false;
        let dump_bytecode_as_base64 = false;
        let generate_struct_layouts: bool = false;
        build::Build::execute_internal(
            rerooted_path.clone(),
            BuildConfig {
                test_mode: true, // make sure to verify tests
                ..build_config.clone()
            },
            with_unpublished_deps,
            legacy_digest,
            dump_bytecode_as_base64,
            generate_struct_layouts,
        )?;
        run_move_unit_tests(
            rerooted_path,
            build_config,
            Some(unit_test_config),
            self.test.compute_coverage,
        )
    }
}

static SET_EXTENSION_HOOK: Lazy<()> =
    Lazy::new(|| set_extension_hook(Box::new(new_testing_object_and_natives_cost_runtime)));

/// This function returns a result of UnitTestResult. The outer result indicates whether it
/// successfully started running the test, and the inner result indicatests whether all tests pass.
pub fn run_move_unit_tests(
    path: PathBuf,
    build_config: BuildConfig,
    config: Option<UnitTestingConfig>,
    compute_coverage: bool,
) -> anyhow::Result<UnitTestResult> {
    // bind the extension hook if it has not yet been done
    Lazy::force(&SET_EXTENSION_HOOK);

    let config = config
        .unwrap_or_else(|| UnitTestingConfig::default_with_bound(Some(MAX_UNIT_TEST_INSTRUCTIONS)));

    move_cli::base::test::run_move_unit_tests(
        &path,
        build_config,
        UnitTestingConfig {
            report_stacktrace_on_abort: true,
            ..config
        },
        sui_move_natives::all_natives(/* silent */ false),
        Some(initial_cost_schedule_for_unit_tests()),
        compute_coverage,
        &mut std::io::stdout(),
    )
}

fn new_testing_object_and_natives_cost_runtime(ext: &mut NativeContextExtensions) {
    let store = InMemoryStorage::new(vec![]);
    let state_view = TemporaryStore::new(
        store,
        InputObjects::new(vec![]),
        TransactionDigest::random(),
        &ProtocolConfig::get_for_min_version(),
    );
    // Use a throwaway metrics registry for testing.
    let registry = prometheus::Registry::new();
    let metrics = Arc::new(LimitsMetrics::new(&registry));

    ext.add(ObjectRuntime::new(
        Box::new(state_view),
        BTreeMap::new(),
        false,
        &ProtocolConfig::get_for_min_version(),
        metrics,
    ));
    ext.add(NativesCostTable::from_protocol_config(
        &ProtocolConfig::get_for_min_version(),
    ));
}
