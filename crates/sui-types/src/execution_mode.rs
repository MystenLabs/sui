// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::TypeTag;

use crate::{
    error::ExecutionError,
    execution::{RawValueType, Value},
    transaction::Argument,
    type_resolver::TypeTagResolver,
};

pub type TransactionIndex = usize;

pub trait ExecutionMode {
    /// All updates to a Arguments used in that Command
    type ArgumentUpdates;
    /// the gathered results from batched executions
    type ExecutionResults;

    /// Controls the calling of arbitrary Move functions
    fn allow_arbitrary_function_calls() -> bool;

    /// Controls the ability to instantiate any Move function parameter with a Pure call arg.
    ///  In other words, you can instantiate any struct or object or other value with its BCS byte
    fn allow_arbitrary_values() -> bool;

    /// If not set, the package ID should be calculated like an object and an
    /// UpgradeCap is produced
    fn packages_are_predefined() -> bool;

    fn empty_arguments() -> Self::ArgumentUpdates;

    fn empty_results() -> Self::ExecutionResults;

    fn add_argument_update(
        resolver: &impl TypeTagResolver,
        acc: &mut Self::ArgumentUpdates,
        arg: Argument,
        _new_value: &Value,
    ) -> Result<(), ExecutionError>;

    fn finish_command(
        resolver: &impl TypeTagResolver,
        acc: &mut Self::ExecutionResults,
        argument_updates: Self::ArgumentUpdates,
        command_result: &[Value],
    ) -> Result<(), ExecutionError>;
}

#[derive(Copy, Clone)]
pub struct Normal;

impl ExecutionMode for Normal {
    type ArgumentUpdates = ();
    type ExecutionResults = ();

    fn allow_arbitrary_function_calls() -> bool {
        false
    }

    fn allow_arbitrary_values() -> bool {
        false
    }

    fn packages_are_predefined() -> bool {
        false
    }

    fn empty_arguments() -> Self::ArgumentUpdates {}

    fn empty_results() -> Self::ExecutionResults {}

    fn add_argument_update(
        _resolver: &impl TypeTagResolver,
        _acc: &mut Self::ArgumentUpdates,
        _arg: Argument,
        _new_value: &Value,
    ) -> Result<(), ExecutionError> {
        Ok(())
    }

    fn finish_command(
        _resolver: &impl TypeTagResolver,
        _acc: &mut Self::ExecutionResults,
        _argument_updates: Self::ArgumentUpdates,
        _command_result: &[Value],
    ) -> Result<(), ExecutionError> {
        Ok(())
    }
}

#[derive(Copy, Clone)]
pub struct Genesis;

impl ExecutionMode for Genesis {
    type ArgumentUpdates = ();
    type ExecutionResults = ();

    fn allow_arbitrary_function_calls() -> bool {
        true
    }

    fn allow_arbitrary_values() -> bool {
        true
    }

    fn packages_are_predefined() -> bool {
        true
    }

    fn empty_arguments() -> Self::ArgumentUpdates {}

    fn empty_results() -> Self::ExecutionResults {}

    fn add_argument_update(
        _resolver: &impl TypeTagResolver,
        _acc: &mut Self::ArgumentUpdates,
        _arg: Argument,
        _new_value: &Value,
    ) -> Result<(), ExecutionError> {
        Ok(())
    }

    fn finish_command(
        _resolver: &impl TypeTagResolver,
        _acc: &mut Self::ExecutionResults,
        _argument_updates: Self::ArgumentUpdates,
        _command_result: &[Value],
    ) -> Result<(), ExecutionError> {
        Ok(())
    }
}

#[derive(Copy, Clone)]
pub struct System;

/// Execution mode for executing a system transaction, including the epoch change
/// transaction and the consensus commit prologue. In this mode, we allow calls to
/// any function bypassing visibility.
impl ExecutionMode for System {
    type ArgumentUpdates = ();
    type ExecutionResults = ();

    fn allow_arbitrary_function_calls() -> bool {
        true
    }

    fn allow_arbitrary_values() -> bool {
        false
    }

    fn packages_are_predefined() -> bool {
        true
    }

    fn empty_arguments() -> Self::ArgumentUpdates {}

    fn empty_results() -> Self::ExecutionResults {}

    fn add_argument_update(
        _resolver: &impl TypeTagResolver,
        _acc: &mut Self::ArgumentUpdates,
        _arg: Argument,
        _new_value: &Value,
    ) -> Result<(), ExecutionError> {
        Ok(())
    }

    fn finish_command(
        _resolver: &impl TypeTagResolver,
        _acc: &mut Self::ExecutionResults,
        _argument_updates: Self::ArgumentUpdates,
        _command_result: &[Value],
    ) -> Result<(), ExecutionError> {
        Ok(())
    }
}

/// WARNING! Using this mode will bypass all normal checks around Move entry functions! This
/// includes the various rules for function arguments, meaning any object can be created just from
/// BCS bytes!
pub struct DevInspect;

pub type ExecutionResult = (
    /*  mutable_reference_outputs */ Vec<(Argument, Vec<u8>, TypeTag)>,
    /*  return_values */ Vec<(Vec<u8>, TypeTag)>,
);

impl ExecutionMode for DevInspect {
    type ArgumentUpdates = Vec<(Argument, Vec<u8>, TypeTag)>;
    type ExecutionResults = Vec<ExecutionResult>;

    fn allow_arbitrary_function_calls() -> bool {
        true
    }

    fn allow_arbitrary_values() -> bool {
        true
    }

    fn packages_are_predefined() -> bool {
        false
    }

    fn empty_arguments() -> Self::ArgumentUpdates {
        vec![]
    }

    fn empty_results() -> Self::ExecutionResults {
        vec![]
    }

    fn add_argument_update(
        resolver: &impl TypeTagResolver,
        acc: &mut Self::ArgumentUpdates,
        arg: Argument,
        new_value: &Value,
    ) -> Result<(), ExecutionError> {
        let (bytes, type_tag) = value_to_bytes_and_tag(resolver, new_value)?;
        acc.push((arg, bytes, type_tag));
        Ok(())
    }

    fn finish_command(
        resolver: &impl TypeTagResolver,
        acc: &mut Self::ExecutionResults,
        argument_updates: Self::ArgumentUpdates,
        command_result: &[Value],
    ) -> Result<(), ExecutionError> {
        let command_bytes = command_result
            .iter()
            .map(|value| value_to_bytes_and_tag(resolver, value))
            .collect::<Result<_, _>>()?;
        acc.push((argument_updates, command_bytes));
        Ok(())
    }
}

fn value_to_bytes_and_tag(
    resolver: &impl TypeTagResolver,
    value: &Value,
) -> Result<(Vec<u8>, TypeTag), ExecutionError> {
    let (type_tag, bytes) = match value {
        Value::Object(obj) => {
            let tag = resolver.get_type_tag(&obj.type_)?;
            let mut bytes = vec![];
            obj.write_bcs_bytes(&mut bytes);
            (tag, bytes)
        }
        Value::Raw(RawValueType::Any, bytes) => {
            // this case shouldn't happen
            (TypeTag::Vector(Box::new(TypeTag::U8)), bytes.clone())
        }
        Value::Raw(RawValueType::Loaded { ty, .. }, bytes) => {
            let tag = resolver.get_type_tag(ty)?;
            (tag, bytes.clone())
        }
    };
    Ok((bytes, type_tag))
}
