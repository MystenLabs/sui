// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::TypeTag;
use std::marker::PhantomData;
use sui_types::error::{ExecutionError, ExecutionErrorTrait};
use sui_types::execution_status::ExecutionFailure;
use sui_types::{execution::ExecutionResult, transaction::Argument};

pub type TransactionIndex = usize;

pub trait ExecutionMode {
    /// All updates to a Arguments used in that Command
    type ArgumentUpdates;
    /// the gathered results from batched executions
    type ExecutionResults;
    /// The error type produced during execution
    type Error: ExecutionErrorTrait;

    /// Controls the calling of arbitrary Move functions
    fn allow_arbitrary_function_calls() -> bool;

    /// Controls the ability to instantiate any Move function parameter with a Pure call arg.
    ///  In other words, you can instantiate any struct or object or other value with its BCS byte
    fn allow_arbitrary_values() -> bool;

    /// Do not perform conservation checks after execution.
    fn skip_conservation_checks() -> bool;

    /// If not set, the package ID should be calculated like an object and an
    /// UpgradeCap is produced
    fn packages_are_predefined() -> bool;

    fn empty_results() -> Self::ExecutionResults;

    const TRACK_EXECUTION: bool;

    fn add_argument_update(
        acc: &mut Self::ArgumentUpdates,
        arg: Argument,
        bytes: Vec<u8>,
        type_: TypeTag,
    ) -> Result<(), ExecutionError>;

    fn finish_command(
        acc: &mut Self::ExecutionResults,
        argument_updates: Vec<(Argument, Vec<u8>, TypeTag)>,
        command_result: Vec<(Vec<u8>, TypeTag)>,
    ) -> Result<(), ExecutionError>;
}

#[derive(Copy, Clone)]
pub struct Normal<E = ExecutionFailure>(PhantomData<fn() -> E>);

impl<E> ExecutionMode for Normal<E>
where
    E: ExecutionErrorTrait,
{
    type ArgumentUpdates = ();
    type ExecutionResults = ();
    type Error = E;

    fn allow_arbitrary_function_calls() -> bool {
        false
    }

    fn allow_arbitrary_values() -> bool {
        false
    }

    fn skip_conservation_checks() -> bool {
        false
    }

    fn packages_are_predefined() -> bool {
        false
    }

    fn empty_results() -> Self::ExecutionResults {}

    const TRACK_EXECUTION: bool = false;

    fn add_argument_update(
        _acc: &mut Self::ArgumentUpdates,
        _arg: Argument,
        _bytes: Vec<u8>,
        _type_: TypeTag,
    ) -> Result<(), ExecutionError> {
        invariant_violation!("should not be called");
    }

    fn finish_command(
        _acc: &mut Self::ExecutionResults,
        _argument_updates: Vec<(Argument, Vec<u8>, TypeTag)>,
        _command_result: Vec<(Vec<u8>, TypeTag)>,
    ) -> Result<(), ExecutionError> {
        invariant_violation!("should not be called");
    }
}

#[derive(Copy, Clone)]
pub struct Genesis;

impl ExecutionMode for Genesis {
    type ArgumentUpdates = ();
    type ExecutionResults = ();
    type Error = ExecutionError;

    fn allow_arbitrary_function_calls() -> bool {
        true
    }

    fn allow_arbitrary_values() -> bool {
        true
    }

    fn packages_are_predefined() -> bool {
        true
    }

    fn skip_conservation_checks() -> bool {
        false
    }

    fn empty_results() -> Self::ExecutionResults {}

    const TRACK_EXECUTION: bool = false;

    fn add_argument_update(
        _acc: &mut Self::ArgumentUpdates,
        _arg: Argument,
        _bytes: Vec<u8>,
        _type_: TypeTag,
    ) -> Result<(), ExecutionError> {
        invariant_violation!("should not be called");
    }

    fn finish_command(
        _acc: &mut Self::ExecutionResults,
        _argument_updates: Vec<(Argument, Vec<u8>, TypeTag)>,
        _command_result: Vec<(Vec<u8>, TypeTag)>,
    ) -> Result<(), ExecutionError> {
        invariant_violation!("should not be called");
    }
}

#[derive(Copy, Clone)]
pub struct System<E = ExecutionError>(PhantomData<fn() -> E>);

/// Execution mode for executing a system transaction, including the epoch change
/// transaction and the consensus commit prologue. In this mode, we allow calls to
/// any function bypassing visibility.
impl<E> ExecutionMode for System<E>
where
    E: ExecutionErrorTrait,
{
    type ArgumentUpdates = ();
    type ExecutionResults = ();
    type Error = E;

    fn allow_arbitrary_function_calls() -> bool {
        // allows bypassing visibility for system calls
        true
    }

    fn allow_arbitrary_values() -> bool {
        // For AuthenticatorStateUpdate, we need to be able to pass in a vector of
        // JWKs, so we need to allow arbitrary values.
        true
    }

    fn skip_conservation_checks() -> bool {
        false
    }

    fn packages_are_predefined() -> bool {
        true
    }

    fn empty_results() -> Self::ExecutionResults {}

    const TRACK_EXECUTION: bool = false;

    fn add_argument_update(
        _acc: &mut Self::ArgumentUpdates,
        _arg: Argument,
        _bytes: Vec<u8>,
        _type_: TypeTag,
    ) -> Result<(), ExecutionError> {
        invariant_violation!("should not be called");
    }

    fn finish_command(
        _acc: &mut Self::ExecutionResults,
        _argument_updates: Vec<(Argument, Vec<u8>, TypeTag)>,
        _command_result: Vec<(Vec<u8>, TypeTag)>,
    ) -> Result<(), ExecutionError> {
        invariant_violation!("should not be called");
    }
}

/// WARNING! Using this mode will bypass all normal checks around Move entry functions! This
/// includes the various rules for function arguments, meaning any object can be created just from
/// BCS bytes!
pub struct DevInspect<const SKIP_ALL_CHECKS: bool>;

impl<const SKIP_ALL_CHECKS: bool> ExecutionMode for DevInspect<SKIP_ALL_CHECKS> {
    type ArgumentUpdates = Vec<(Argument, Vec<u8>, TypeTag)>;
    type ExecutionResults = Vec<ExecutionResult>;
    type Error = ExecutionError;

    fn allow_arbitrary_function_calls() -> bool {
        SKIP_ALL_CHECKS
    }

    fn allow_arbitrary_values() -> bool {
        SKIP_ALL_CHECKS
    }

    fn skip_conservation_checks() -> bool {
        SKIP_ALL_CHECKS
    }

    fn packages_are_predefined() -> bool {
        false
    }

    fn empty_results() -> Self::ExecutionResults {
        vec![]
    }

    const TRACK_EXECUTION: bool = true;

    fn add_argument_update(
        acc: &mut Self::ArgumentUpdates,
        arg: Argument,
        bytes: Vec<u8>,
        type_: TypeTag,
    ) -> Result<(), ExecutionError> {
        acc.push((arg, bytes, type_));
        Ok(())
    }

    fn finish_command(
        acc: &mut Self::ExecutionResults,
        argument_updates: Vec<(Argument, Vec<u8>, TypeTag)>,
        command_result: Vec<(Vec<u8>, TypeTag)>,
    ) -> Result<(), ExecutionError> {
        acc.push((argument_updates, command_result));
        Ok(())
    }
}
