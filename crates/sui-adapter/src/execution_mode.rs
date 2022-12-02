// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_vm_runtime::session::SerializedReturnValues;
use sui_types::error::ExecutionError;

pub type TransactionIndex = usize;

pub trait ExecutionMode {
    type ExecutionResult;
    type ExecutionResults;

    fn allow_arbitrary_function_calls() -> bool;

    fn make_result(
        return_values: &SerializedReturnValues,
    ) -> Result<Self::ExecutionResult, ExecutionError>;

    fn empty_results() -> Self::ExecutionResults;

    fn add_result(
        results: &mut Self::ExecutionResults,
        idx: TransactionIndex,
        result: Self::ExecutionResult,
    );
}

pub struct Normal;

impl ExecutionMode for Normal {
    type ExecutionResult = ();
    type ExecutionResults = ();

    fn allow_arbitrary_function_calls() -> bool {
        false
    }

    fn make_result(srv: &SerializedReturnValues) -> Result<Self::ExecutionResult, ExecutionError> {
        assert_invariant!(srv.return_values.is_empty(), "Return values must be empty");
        Ok(())
    }

    fn empty_results() -> Self::ExecutionResults {
        ()
    }

    fn add_result(_: &mut Self::ExecutionResults, _: TransactionIndex, _: Self::ExecutionResult) {
        ()
    }
}

pub struct DevInspect;

impl ExecutionMode for DevInspect {
    type ExecutionResult = SerializedReturnValues;
    type ExecutionResults = Vec<(TransactionIndex, SerializedReturnValues)>;

    fn allow_arbitrary_function_calls() -> bool {
        true
    }

    fn make_result(srv: &SerializedReturnValues) -> Result<Self::ExecutionResult, ExecutionError> {
        let SerializedReturnValues {
            mutable_reference_outputs,
            return_values,
        } = srv;
        Ok(SerializedReturnValues {
            mutable_reference_outputs: mutable_reference_outputs.clone(),
            return_values: return_values.clone(),
        })
    }

    fn empty_results() -> Self::ExecutionResults {
        todo!()
    }

    fn add_result(
        results: &mut Self::ExecutionResults,
        idx: TransactionIndex,
        result: Self::ExecutionResult,
    ) {
        results.push((idx, result))
    }
}
