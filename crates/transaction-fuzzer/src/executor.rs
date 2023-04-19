// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_core::{
    authority::AuthorityState,
    test_utils::{init_state, send_and_confirm_transaction},
};
use sui_types::{
    error::SuiError,
    messages::{
        ExecutionFailureStatus, ExecutionStatus, TransactionEffectsAPI, VerifiedTransaction,
    },
    object::Object,
};
use tokio::runtime::Runtime;

pub type ExecutionResult = Result<ExecutionStatus, SuiError>;

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

pub struct Executor {
    pub state: Arc<AuthorityState>,
    pub rt: Runtime,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor {
    pub fn new() -> Self {
        let rt = Runtime::new().unwrap();
        let state = rt.block_on(init_state());
        Self { state, rt }
    }

    pub fn add_object(&mut self, object: Object) {
        self.rt.block_on(self.state.insert_genesis_object(object));
    }

    pub fn execute_transaction(&mut self, txn: VerifiedTransaction) -> ExecutionResult {
        self.rt
            .block_on(send_and_confirm_transaction(&self.state, None, txn))
            .map(|(_, effects)| effects.into_data().status().clone())
    }

    pub fn execute_transactions(
        &mut self,
        txn: impl IntoIterator<Item = VerifiedTransaction>,
    ) -> Vec<ExecutionResult> {
        txn.into_iter()
            .map(|txn| self.execute_transaction(txn))
            .collect()
    }
}
