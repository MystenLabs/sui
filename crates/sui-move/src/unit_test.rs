// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::{
    self,
    test::{self, UnitTestResult},
};
use move_package::BuildConfig;
use move_unit_test::{extensions::set_extension_hook, UnitTestingConfig};
use move_vm_runtime::native_extensions::NativeContextExtensions;
use once_cell::sync::Lazy;
use std::{cell::RefCell, collections::BTreeMap, path::Path, rc::Rc, sync::Arc};
use sui_move_build::{decorate_warnings, implicit_deps};
use sui_move_natives::{
    object_runtime::ObjectRuntime, test_scenario::InMemoryTestStore,
    transaction_context::TransactionContext, NativesCostTable,
};
use sui_package_management::system_package_versions::latest_system_packages;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{SuiAddress, TxContext},
    digests::TransactionDigest,
    gas_model::tables::initial_cost_schedule_for_unit_tests,
    in_memory_storage::InMemoryStorage,
    metrics::LimitsMetrics,
};

// Move unit tests will halt after executing this many steps. This is a protection to avoid divergence
const MAX_UNIT_TEST_INSTRUCTIONS: u64 = 1_000_000;

#[derive(Parser)]
#[group(id = "sui-move-test")]
pub struct Test {
    #[clap(flatten)]
    pub test: test::Test,
}

impl Test {
    pub fn execute(
        self,
        path: Option<&Path>,
        build_config: BuildConfig,
    ) -> anyhow::Result<UnitTestResult> {
        let compute_coverage = self.test.compute_coverage;
        if !cfg!(feature = "tracing") && compute_coverage {
            return Err(anyhow::anyhow!(
                "The --coverage flag is currently supported only in builds built with the `tracing` feature enabled. \
                Please build the Sui CLI from source with `--features tracing` to use this flag."
            ));
        }
        // save disassembly if trace execution is enabled
        let save_disassembly = self.test.trace_execution.is_some();
        // find manifest file directory from a given path or (if missing) from current dir
        let rerooted_path = base::reroot_path(path)?;
        let unit_test_config = self.test.unit_test_config();
        run_move_unit_tests(
            &rerooted_path,
            build_config,
            Some(unit_test_config),
            compute_coverage,
            save_disassembly,
        )
    }
}

// Create a separate test store per-thread.
thread_local! {
    static TEST_STORE_INNER: RefCell<InMemoryStorage> = RefCell::new(InMemoryStorage::default());
}

static TEST_STORE: Lazy<InMemoryTestStore> = Lazy::new(|| InMemoryTestStore(&TEST_STORE_INNER));

static SET_EXTENSION_HOOK: Lazy<()> =
    Lazy::new(|| set_extension_hook(Box::new(new_testing_object_and_natives_cost_runtime)));

/// This function returns a result of UnitTestResult. The outer result indicates whether it
/// successfully started running the test, and the inner result indicatests whether all tests pass.
pub fn run_move_unit_tests(
    path: &Path,
    mut build_config: BuildConfig,
    config: Option<UnitTestingConfig>,
    compute_coverage: bool,
    save_disassembly: bool,
) -> anyhow::Result<UnitTestResult> {
    // bind the extension hook if it has not yet been done
    Lazy::force(&SET_EXTENSION_HOOK);

    let config = config
        .unwrap_or_else(|| UnitTestingConfig::default_with_bound(Some(MAX_UNIT_TEST_INSTRUCTIONS)));
    build_config.implicit_dependencies = implicit_deps(latest_system_packages());

    let result = move_cli::base::test::run_move_unit_tests(
        path,
        build_config,
        UnitTestingConfig {
            report_stacktrace_on_abort: true,
            ..config
        },
        sui_move_natives::all_natives(
            /* silent */ false,
            &ProtocolConfig::get_for_max_version_UNSAFE(),
        ),
        Some(initial_cost_schedule_for_unit_tests()),
        compute_coverage,
        save_disassembly,
        &mut std::io::stdout(),
    );
    result.map(|(test_result, warning_diags)| {
        if test_result == UnitTestResult::Success {
            if let Some(diags) = warning_diags {
                decorate_warnings(diags, None);
            }
        }
        test_result
    })
}

fn new_testing_object_and_natives_cost_runtime(ext: &mut NativeContextExtensions) {
    // Use a throwaway metrics registry for testing.
    let registry = prometheus::Registry::new();
    let metrics = Arc::new(LimitsMetrics::new(&registry));
    let store = Lazy::force(&TEST_STORE);
    let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();

    ext.add(ObjectRuntime::new(
        store,
        BTreeMap::new(),
        false,
        Box::leak(Box::new(ProtocolConfig::get_for_max_version_UNSAFE())), // leak for testing
        metrics,
        0, // epoch id
    ));
    ext.add(NativesCostTable::from_protocol_config(&protocol_config));
    let tx_context = TxContext::new_from_components(
        &SuiAddress::ZERO,
        &TransactionDigest::default(),
        &0,
        0,
        0,
        0,
        None,
        &protocol_config,
    );
    ext.add(TransactionContext::new_for_testing(Rc::new(RefCell::new(
        tx_context,
    ))));
    ext.add(store);
}
