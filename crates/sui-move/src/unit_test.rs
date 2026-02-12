// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::{
    self,
    test::{self, UnitTestResult},
};
use move_package_alt_compilation::build_config::BuildConfig;
use move_unit_test::{UnitTestingConfig, vm_test_setup::VMTestSetup};
use move_vm_config::runtime::VMConfig;
use move_vm_runtime::native_extensions::NativeContextExtensions;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    path::Path,
    rc::Rc,
    sync::{Arc, LazyLock},
};
use sui_adapter::gas_meter::SuiGasMeter;
use sui_move_build::decorate_warnings;
use sui_move_natives::{
    NativesCostTable, object_runtime::ObjectRuntime, test_scenario::InMemoryTestStore,
    transaction_context::TransactionContext,
};
use sui_package_alt::find_environment;
use sui_protocol_config::ProtocolConfig;
use sui_sdk::wallet_context::WalletContext;
use sui_types::{
    base_types::{SuiAddress, TxContext},
    digests::TransactionDigest,
    gas::{SuiGasStatus, SuiGasStatusAPI},
    gas_model::{tables::GasStatus, units_types::Gas},
    in_memory_storage::InMemoryStorage,
    metrics::LimitsMetrics,
};

// Move unit tests will halt after executing this many steps. This is a protection to avoid divergence
pub static MAX_UNIT_TEST_INSTRUCTIONS: LazyLock<u64> =
    LazyLock::new(|| ProtocolConfig::get_for_max_version_UNSAFE().max_tx_gas());

/// Gas price used for the meter during Move unit tests.
const TEST_GAS_PRICE: u64 = 500;

#[derive(Parser)]
#[group(id = "sui-move-test")]
pub struct Test {
    #[clap(flatten)]
    pub test: test::Test,
}

impl Test {
    pub async fn execute(
        self,
        path: Option<&Path>,
        mut build_config: BuildConfig,
        wallet: &WalletContext,
    ) -> anyhow::Result<UnitTestResult> {
        let compute_coverage = self.test.compute_coverage;
        if !cfg!(feature = "tracing") && compute_coverage {
            return Err(anyhow::anyhow!(
                "The --coverage flag is currently supported only in builds built with the `tracing` feature enabled. \
                Please build the Sui CLI from source with `--features tracing` to use this flag."
            ));
        }
        // save disassembly if trace execution is enabled
        let save_disassembly = self.test.trace;
        // set the default flavor to Sui if not already set by the user
        if build_config.default_flavor.is_none() {
            build_config.default_flavor = Some(move_compiler::editions::Flavor::Sui);
        }

        // find manifest file directory from a given path or (if missing) from current dir
        let rerooted_path = base::reroot_path(path)?;
        let unit_test_config = self.test.unit_test_config();

        // set the environment (this is a little janky: we get it from the manifest here, then pass
        // it as the optional argument in the build-config, which then looks it up again, but it
        // should be ok.
        let environment =
            find_environment(&rerooted_path, build_config.environment, wallet, false).await?;
        build_config.environment = Some(environment.name);

        run_move_unit_tests(
            &rerooted_path,
            build_config,
            Some(unit_test_config),
            compute_coverage,
            save_disassembly,
        )
        .await
    }
}

/// This function returns a result of UnitTestResult. The outer result indicates whether it
/// successfully started running the test, and the inner result indicatests whether all tests pass.
pub async fn run_move_unit_tests(
    path: &Path,
    build_config: BuildConfig,
    config: Option<UnitTestingConfig>,
    compute_coverage: bool,
    save_disassembly: bool,
) -> anyhow::Result<UnitTestResult> {
    let config = config.unwrap_or_else(|| {
        UnitTestingConfig::default_with_bound(Some(*MAX_UNIT_TEST_INSTRUCTIONS))
    });

    let result = move_cli::base::test::run_move_unit_tests::<sui_package_alt::SuiFlavor, _, _>(
        path,
        build_config,
        UnitTestingConfig {
            report_stacktrace_on_abort: true,
            ..config
        },
        SuiVMTestSetup::new(),
        compute_coverage,
        save_disassembly,
        &mut std::io::stdout(),
    )
    .await;

    result.map(|(test_result, warning_diags)| {
        if test_result == UnitTestResult::Success
            && let Some(diags) = warning_diags
        {
            decorate_warnings(diags, None);
        }
        test_result
    })
}

pub struct SuiVMTestSetup {
    gas_price: u64,
    reference_gas_price: u64,
    protocol_config: ProtocolConfig,
    native_function_table: move_vm_runtime::native_functions::NativeFunctionTable,
}

impl Default for SuiVMTestSetup {
    fn default() -> Self {
        Self::new()
    }
}

impl SuiVMTestSetup {
    pub fn new() -> Self {
        let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        let native_function_table =
            sui_move_natives::all_natives(/* silent */ false, &protocol_config);
        Self {
            gas_price: TEST_GAS_PRICE,
            reference_gas_price: TEST_GAS_PRICE,
            protocol_config,
            native_function_table,
        }
    }

    pub fn max_gas_budget(&self) -> u64 {
        self.protocol_config.max_tx_gas()
    }
}

impl VMTestSetup for SuiVMTestSetup {
    type Meter<'a> = SuiGasMeter<SuiGasStatusTestWrapper>;
    type ExtensionsBuilder<'a> = InMemoryTestStore;

    fn new_meter<'a>(&'a self, execution_bound: Option<u64>) -> Self::Meter<'a> {
        SuiGasMeter(SuiGasStatusTestWrapper(
            SuiGasStatus::new(
                execution_bound.unwrap_or(*MAX_UNIT_TEST_INSTRUCTIONS),
                self.gas_price,
                self.reference_gas_price,
                &self.protocol_config,
            )
            .unwrap(),
        ))
    }

    fn used_gas<'a>(&'a self, execution_bound: u64, meter: Self::Meter<'a>) -> u64 {
        let gas_status = &meter.0;
        Gas::new(execution_bound)
            .checked_sub(gas_status.remaining_gas())
            .unwrap()
            .into()
    }

    fn vm_config(&self) -> VMConfig {
        sui_adapter::adapter::vm_config(&self.protocol_config)
    }

    fn native_function_table(&self) -> move_vm_runtime::native_functions::NativeFunctionTable {
        self.native_function_table.clone()
    }

    fn new_extensions_builder(&self) -> InMemoryTestStore {
        InMemoryTestStore(RefCell::new(InMemoryStorage::default()))
    }

    fn new_native_context_extensions<'ext>(
        &self,
        store: &'ext InMemoryTestStore,
    ) -> NativeContextExtensions<'ext> {
        let mut ext = NativeContextExtensions::default();
        // Use a throwaway metrics registry for testing.
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(LimitsMetrics::new(&registry));

        ext.add(ObjectRuntime::new(
            store,
            BTreeMap::new(),
            false,
            Box::leak(Box::new(ProtocolConfig::get_for_max_version_UNSAFE())), // leak for testing
            metrics,
            0, // epoch id
        ));
        ext.add(NativesCostTable::from_protocol_config(
            &self.protocol_config,
        ));
        let tx_context = TxContext::new_from_components(
            &SuiAddress::ZERO,
            &TransactionDigest::default(),
            &0,
            0,
            0,
            0,
            0,
            None,
            &self.protocol_config,
        );
        ext.add(TransactionContext::new_for_testing(Rc::new(RefCell::new(
            tx_context,
        ))));
        ext.add(store);
        ext
    }
}

// Massaging to get traits to line up.
pub struct SuiGasStatusTestWrapper(SuiGasStatus);

impl Deref for SuiGasStatusTestWrapper {
    type Target = GasStatus;

    fn deref(&self) -> &Self::Target {
        self.0.move_gas_status()
    }
}

impl DerefMut for SuiGasStatusTestWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.move_gas_status_mut()
    }
}
