// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{
    Deserialize,
    de::{self, Deserializer},
    ser::Serializer,
};
use serde_json::Value as JsonValue;
use starlark::{
    ErrorKind,
    environment::{Globals, Module},
    eval::Evaluator,
    syntax::{AstModule, Dialect, DialectTypes},
    values::{AllocValue, Heap, Value, dict::AllocDict},
};
use sui_types::{
    base_types::ObjectRef,
    signature::GenericSignature,
    transaction::{InputObjectKind, TransactionData},
};
use tracing::warn;

/// The name of the input variables that the transaction will have in the Starlark program.
const TX_DATA_NAME: &str = "tx_data";
const TX_SIGNERS_NAME: &str = "tx_signers";
const TX_INPUT_OBJECTS_NAME: &str = "tx_input_objects";
const TX_RECEIVING_OBJECTS_NAME: &str = "tx_receiving_objects";
const TX_DIGEST_NAME: &str = "tx_digest";

/// The dummy name of the Starlark file being executed. We will just be passing the string of the
/// program in directly so this is not important but may appear in error messages.
const STAR_INPUT_FILE_NAME: &str = "dynamic_transaction_signing_checks.star";

#[derive(Debug, thiserror::Error)]
pub enum DynamicCheckRunnerError {
    #[error("Failed to serialize transaction data to JSON: {0}")]
    JSONSerializationError(String),
    #[error("Failed to parse Starlark program value -- unsupported number type {0}")]
    UnsupportedNumberFormat(String),
    #[error(
        "Failed to execute Starlark program -- invalid return type expected a bool but got {0}"
    )]
    InvalidReturnType(String),
    #[error("Failed to execute Starlark program: {0}")]
    ExecutionError(ErrorKind),
    #[error("Failed to load Starlark program: {0}")]
    LoadingError(ErrorKind),
    #[error("Check failed -- transaction denied")]
    CheckFailure,
}

#[derive(Debug, Clone)]
pub struct DynamicCheckRunnerContext {
    module: AstModule,
    globals: Globals,
    loaded_program: String,
}

const DIALECT: Dialect = Dialect {
    enable_def: true,
    enable_lambda: true,
    enable_keyword_only_arguments: false,
    enable_positional_only_arguments: false,
    enable_types: DialectTypes::Disable,
    // NB: set loader to false to prevent any external loading
    enable_load: false,
    enable_load_reexport: false,
    // NB: Allow for top level statements to be used (e.g., top-level `for`, `if`, etc.)
    enable_top_level_stmt: true,
    enable_f_strings: false,
    // NB: We explicitly fully initalize the struct to prevent any future changes to the dialect
    // without us noticing and deciding whether or not the new feature should be enabled.
    _non_exhaustive: (),
};

impl DynamicCheckRunnerContext {
    /// Create a new `DynamicCheckRunnerContext` with the given Starlark program
    /// `starlark_program` string. This will parse and validate the program is syntactically
    /// correct and will set up shared (immutable) state that can be reused. This will not
    /// run the program.
    ///
    /// The `starlark_program` string should be a valid Starlark program that returns a boolean
    /// value when run -- `True` in the case that the transaction should be allowed, or `False` if
    /// the transaction should be denied. Any other return value other than `True` (including
    /// errors) should be considered a denial.
    pub fn new(starlark_program: String) -> Result<Self, DynamicCheckRunnerError> {
        // Adds global functions and variables to the dialect (e.g., True, False, Maps, Lists, etc.)
        // The full spec of what exactly is added here can be found here:
        // https://github.com/bazelbuild/starlark/blob/master/spec.md#built-in-constants-and-functions
        let globals = Globals::standard();
        warn!(
            "Dynamic transaction checks are enabled. Make sure that you intend to be running \
            dynamic checks on transactions."
        );
        let module = AstModule::parse(STAR_INPUT_FILE_NAME, starlark_program.clone(), &DIALECT)
            .map_err(|e| DynamicCheckRunnerError::LoadingError(e.into_kind()))?;
        Ok(Self {
            module,
            globals,
            loaded_program: starlark_program,
        })
    }

    /// Run the Starlark program in `self` with the given transaction data, signatures, input
    /// object kinds, and receiving objects.
    pub fn run_predicate(
        &self,
        tx_data: &TransactionData,
        tx_signatures: &[GenericSignature],
        input_object_kinds: &[InputObjectKind],
        receiving_objects: &[ObjectRef],
    ) -> Result<(), DynamicCheckRunnerError> {
        let tx_data_json = serde_json::to_value(tx_data)
            .map_err(|e| DynamicCheckRunnerError::JSONSerializationError(e.to_string()))?;
        let tx_signatures_json = serde_json::to_value(tx_signatures)
            .map_err(|e| DynamicCheckRunnerError::JSONSerializationError(e.to_string()))?;
        let input_object_kinds_json = serde_json::to_value(input_object_kinds)
            .map_err(|e| DynamicCheckRunnerError::JSONSerializationError(e.to_string()))?;
        let receiving_objects_json = serde_json::to_value(receiving_objects)
            .map_err(|e| DynamicCheckRunnerError::JSONSerializationError(e.to_string()))?;
        let digest_json = serde_json::to_value(tx_data.digest())
            .map_err(|e| DynamicCheckRunnerError::JSONSerializationError(e.to_string()))?;

        self.run_starlark_predicate(
            &tx_data_json,
            &tx_signatures_json,
            &input_object_kinds_json,
            &receiving_objects_json,
            &digest_json,
        )
    }

    fn run_starlark_predicate(
        &self,
        tx_data: &JsonValue,
        tx_signatures: &JsonValue,
        tx_input_object_kinds: &JsonValue,
        tx_receiving_objects: &JsonValue,
        tx_digest: &JsonValue,
    ) -> Result<(), DynamicCheckRunnerError> {
        let heap = Heap::new();
        let env = Module::new();

        let tx_data_value = Self::json_to_starlark(tx_data, &heap)?;
        let tx_signers_value = Self::json_to_starlark(tx_signatures, &heap)?;
        let tx_input_object_kinds_value = Self::json_to_starlark(tx_input_object_kinds, &heap)?;
        let tx_receiving_objects_value = Self::json_to_starlark(tx_receiving_objects, &heap)?;
        let tx_digest_value = Self::json_to_starlark(tx_digest, &heap)?;

        env.set(TX_DATA_NAME, tx_data_value);
        env.set(TX_SIGNERS_NAME, tx_signers_value);
        env.set(TX_INPUT_OBJECTS_NAME, tx_input_object_kinds_value);
        env.set(TX_RECEIVING_OBJECTS_NAME, tx_receiving_objects_value);
        env.set(TX_DIGEST_NAME, tx_digest_value);

        let mut evaluator = Evaluator::new(&env);
        let output_value = evaluator
            .eval_module(self.module.clone(), &self.globals)
            .map_err(|e| DynamicCheckRunnerError::ExecutionError(e.into_kind()))?;
        let transaction_allowed = output_value
            .unpack_bool()
            .ok_or_else(|| DynamicCheckRunnerError::InvalidReturnType(output_value.to_repr()))?;
        if transaction_allowed {
            Ok(())
        } else {
            Err(DynamicCheckRunnerError::CheckFailure)
        }
    }

    fn json_to_starlark<'v>(
        value: &JsonValue,
        heap: &'v Heap,
    ) -> Result<Value<'v>, DynamicCheckRunnerError> {
        Ok(match value {
            JsonValue::Null => Value::new_none(),
            JsonValue::Bool(b) => Value::new_bool(*b),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_u64() {
                    heap.alloc(i)
                } else {
                    return Err(DynamicCheckRunnerError::UnsupportedNumberFormat(
                        n.to_string(),
                    ));
                }
            }
            JsonValue::String(s) => heap.alloc(s.as_str()),
            JsonValue::Array(arr) => {
                let list: Vec<_> = arr
                    .iter()
                    .map(|v| Self::json_to_starlark(v, heap))
                    .collect::<Result<_, _>>()?;
                list.alloc_value(heap)
            }
            JsonValue::Object(obj) => {
                let kvs: Vec<_> = obj
                    .iter()
                    .map(|(k, v)| {
                        let key = heap.alloc(k.as_str());
                        let val = Self::json_to_starlark(v, heap)?;
                        Ok((key, val))
                    })
                    .collect::<Result<_, _>>()?;
                heap.alloc(AllocDict(kvs))
            }
        })
    }
}

// Custom serialization/deserialization for the `DynamicCheckRunnerContext` struct. This allows us
// to validate the program at the time that it is deserialized, rather than at the time that it is
// first used. This provides better error message locality and allows us to fail fast if the
// program is invalid. We keep the invariant here that `serialize(deserialize(program)) ==
// program`.

/// Deserialize a `DynamicCheckRunnerContext` from a string. This will parse the string as a
/// Starlark program and validate that it is syntactically correct and setup the
/// `DynamicCheckRunnerContext` for it. If the program is syntactically invalid, an error will be
/// returned.
pub(crate) fn deserialize_dynamic_transaction_checks<'de, D>(
    deserializer: D,
) -> Result<Option<DynamicCheckRunnerContext>, D::Error>
where
    D: Deserializer<'de>,
{
    let path_opt: Option<String> = Option::deserialize(deserializer)?;
    match path_opt {
        Some(p) => Ok(Some(
            DynamicCheckRunnerContext::new(p).map_err(de::Error::custom)?,
        )),
        None => Ok(None),
    }
}

/// Takes a `DynamicCheckRunnerContext` and serializes the original source program as the returned
/// string. No parsed state or otherwise is serialized.
pub(crate) fn serialize_dynamic_transaction_checks<S>(
    value: &Option<DynamicCheckRunnerContext>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(DynamicCheckRunnerContext { loaded_program, .. }) => {
            serializer.serialize_some(&loaded_program)
        }
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn parse_on_load_invalid() {
        let program = r#"
            def main(): return 1
        "#;
        let result = super::DynamicCheckRunnerContext::new(program.to_string());
        assert!(result.is_err());
    }

    #[test]
    fn parse_on_load_valid() {
        let program = r#"
def main(): 
    return 1
        "#;
        let result = super::DynamicCheckRunnerContext::new(program.to_string());
        assert!(result.is_ok());
    }
}
