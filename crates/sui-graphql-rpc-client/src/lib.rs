// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Value;
use reqwest::header::ToStrError;
use serde_json::Number;

pub mod response;
pub mod simple_client;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Service version header not found")]
    ServiceVersionHeaderNotFound,
    #[error("Service version header value invalid string: {error}")]
    ServiceVersionHeaderValueInvalidString { error: ToStrError },
    #[error("Invalid usage number for {usage_name}: {usage_number}")]
    InvalidUsageNumber {
        usage_name: String,
        usage_number: Number,
    },
    #[error("Invalid usage field for {usage_name}: {usage_value}")]
    InvalidUsageValue {
        usage_name: String,
        usage_value: Value,
    },
    #[error("{item_type} at pos {idx} must not be empty")]
    InvalidEmptyItem { item_type: String, idx: usize },
    #[error(
        "Invalid variable name: `{var_name}`. Variable names must be non-empty and start with a letter or underscore, and may only contain letters, digits, and underscores."
    )]
    InvalidVariableName { var_name: String },

    #[error(
        "Conflicting type definitions for variable {var_name}: {var_type_prev} vs {var_type_curr}"
    )]
    VariableDefinitionConflict {
        var_name: String,
        var_type_prev: String,
        var_type_curr: String,
    },
    #[error("Conflicting values for variable {var_name}: {var_val_prev} vs {var_val_curr}")]
    VariableValueConflict {
        var_name: String,
        var_val_prev: serde_json::Value,
        var_val_curr: serde_json::Value,
    },
    #[error(transparent)]
    InnerClientError(#[from] reqwest::Error),
}
