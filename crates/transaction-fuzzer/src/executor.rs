// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fmt::Debug, path::PathBuf, sync::Arc};

use sui_core::authority::test_authority_builder::TestAuthorityBuilder;
use sui_core::{authority::AuthorityState, test_utils::send_and_confirm_transaction};
use sui_move_build::BuildConfig;
use sui_types::base_types::ObjectID;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::error::SuiError;
use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};
use sui_types::object::Object;
use sui_types::transaction::{Transaction, TransactionData};
use sui_types::utils::to_sender_signed_transaction;
use tokio::runtime::Runtime;

use crate::account_universe::{AccountCurrent, PUBLISH_BUDGET};

pub type ExecutionResult = Result<ExecutionStatus, SuiError>;

fn build_test_modules(test_dir: &str) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["data", test_dir]);
    let with_unpublished_deps = false;
    let config = BuildConfig::new_for_testing();
    let package = config.build(&path).unwrap();
    (
        package.get_package_digest(with_unpublished_deps).to_vec(),
        package.get_package_bytes(with_unpublished_deps),
    )
}

// We want to look for either panics (in which case we won't hit this) or invariant violations in
// which case we want to panic.
pub fn assert_is_acceptable_result(result: &ExecutionResult) {
    if let Ok(
        e @ ExecutionStatus::Failure {
            error: ExecutionFailureStatus::InvariantViolation,
            command: _,
        },
    ) = result
    {
        panic!("Invariant violation: {e:#?}")
    }
}

#[derive(Clone)]
pub struct Executor {
    pub state: Arc<AuthorityState>,
    pub rt: Arc<Runtime>,
}

impl Debug for Executor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Executor").finish()
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor {
    pub fn new() -> Self {
        let rt = Runtime::new().unwrap();
        let state = rt.block_on(TestAuthorityBuilder::new().build());
        Self {
            state,
            rt: Arc::new(rt),
        }
    }

    pub fn new_with_rgp(rgp: u64) -> Self {
        let rt = Runtime::new().unwrap();
        let state = rt.block_on(
            TestAuthorityBuilder::new()
                .with_reference_gas_price(rgp)
                .build(),
        );
        Self {
            state,
            rt: Arc::new(rt),
        }
    }

    pub fn get_reference_gas_price(&self) -> u64 {
        self.state.reference_gas_price_for_testing().unwrap()
    }

    pub fn add_object(&mut self, object: Object) {
        self.rt.block_on(self.state.insert_genesis_object(object));
    }

    pub fn add_objects(&mut self, objects: &[Object]) {
        self.rt.block_on(self.state.insert_genesis_objects(objects));
    }

    pub fn execute_transaction(&mut self, txn: Transaction) -> ExecutionResult {
        self.rt
            .block_on(send_and_confirm_transaction(&self.state, None, txn))
            .map(|(_, effects)| effects.into_data().status().clone())
    }

    pub fn publish(
        &mut self,
        package_name: &str,
        dep_ids: Vec<ObjectID>,
        account: &mut AccountCurrent,
    ) -> TransactionEffects {
        let (_, modules) = build_test_modules(package_name);
        // let gas_obj_ref = account.current_coins.last().unwrap().compute_object_reference();
        let gas_object = account.new_gas_object(self);
        let data = TransactionData::new_module(
            account.initial_data.account.address,
            gas_object.compute_object_reference(),
            modules,
            dep_ids,
            PUBLISH_BUDGET,
            1000,
        );
        let txn = to_sender_signed_transaction(data, &account.initial_data.account.key);
        let effects = self
            .rt
            .block_on(send_and_confirm_transaction(&self.state, None, txn))
            .unwrap()
            .1
            .into_data();

        assert!(
            matches!(effects.status(), ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status()
        );
        effects
    }

    pub fn execute_transactions(
        &mut self,
        txn: impl IntoIterator<Item = Transaction>,
    ) -> Vec<ExecutionResult> {
        txn.into_iter()
            .map(|txn| self.execute_transaction(txn))
            .collect()
    }
}
