// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_binary_format::errors::VMError;
use move_core_types::account_address::AccountAddress;
use sui_types::TypeTag;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum Error {
    #[error("{0}")]
    Bcs(#[from] bcs::Error),

    #[error("Store {} error: {}", store, error)]
    Store { store: &'static str, error: String },

    #[error("{0}")]
    Deserialize(VMError),

    #[error(
        "Package has no modules: {}",
        .0.to_canonical_display(/* with_prefix */ true),
    )]
    EmptyPackage(AccountAddress),

    #[error(
        "Function not found: {}::{1}::{2}",
        .0.to_canonical_display(/* with_prefix */ true),
    )]
    FunctionNotFound(AccountAddress, String, String),

    #[error(
        "Conflicting types for input {0}: {} and {}",
        .1.to_canonical_display(/* with_prefix */ true),
        .2.to_canonical_display(/* with_prefix */ true),
    )]
    InputTypeConflict(u16, TypeTag, TypeTag),

    #[error(
        "Linkage not found for package: {}",
        .0.to_canonical_display(/* with_prefix */ true),
    )]
    LinkageNotFound(AccountAddress),

    #[error(
        "Module not found: {}::{1}",
        .0.to_canonical_display(/* with_prefix */ true),
    )]
    ModuleNotFound(AccountAddress, String),

    #[error(
        "No origin package found for {}::{1}::{2}",
        .0.to_canonical_display(/* with_prefix */ true),
    )]
    NoTypeOrigin(AccountAddress, String, String),

    #[error(
        "Not a package: {}",
        .0.to_canonical_display(/* with_prefix */ true),
    )]
    NotAPackage(AccountAddress),

    #[error("Not an identifier: '{0}'")]
    NotAnIdentifier(String),

    #[error(
        "Package not found: {}",
        .0.to_canonical_display(/* with_prefix */ true),
    )]
    PackageNotFound(AccountAddress),

    #[error(
        "Datatype not found: {}::{1}::{2}",
        .0.to_canonical_display(/* with_prefix */ true),
    )]
    DatatypeNotFound(AccountAddress, String, String),

    #[error("More than {0} struct definitions required to resolve type")]
    TooManyTypeNodes(usize, usize),

    #[error("Expected at most {0} type parameters, got {1}")]
    TooManyTypeParams(usize, usize),

    #[error("Expected {0} type parameters, but got {1}")]
    TypeArityMismatch(usize, usize),

    #[error("Type parameter nesting exceeded limit of {0}")]
    TypeParamNesting(usize, usize),

    #[error("Type Parameter {0} out of bounds ({1})")]
    TypeParamOOB(u16, usize),

    #[error("Unexpected reference type.")]
    UnexpectedReference,

    #[error("Unexpected type: 'signer'.")]
    UnexpectedSigner,

    #[error("Unexpected error: {0}")]
    UnexpectedError(Arc<dyn std::error::Error + Send + Sync + 'static>),

    #[error("Type layout nesting exceeded limit of {0}")]
    ValueNesting(usize),
}
