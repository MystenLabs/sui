// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::messages::CommandIndex;
use crate::ObjectID;
use move_binary_format::file_format::{CodeOffset, TypeParameterIndex};
use move_core_types::language_storage::ModuleId;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use thiserror::Error;

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Success,
    /// Gas used in the failed case, and the error.
    Failure {
        /// The error
        error: ExecutionFailureStatus,
        /// Which command the error occurred
        command: Option<CommandIndex>,
    },
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error)]
pub enum ExecutionFailureStatus {
    //
    // General transaction errors
    //
    #[error("Insufficient Gas.")]
    InsufficientGas,
    #[error("Invalid Gas Object. Possibly not address-owned or possibly not a SUI coin.")]
    InvalidGasObject,
    #[error("INVARIANT VIOLATION.")]
    InvariantViolation,
    #[error("Attempted to used feature that is not supported yet")]
    FeatureNotYetSupported,
    #[error(
        "Move object with size {object_size} is larger \
        than the maximum object size {max_object_size}"
    )]
    MoveObjectTooBig {
        object_size: u64,
        max_object_size: u64,
    },
    #[error(
        "Move package with size {object_size} is larger than the \
        maximum object size {max_object_size}"
    )]
    MovePackageTooBig {
        object_size: u64,
        max_object_size: u64,
    },
    #[error("Circular Object Ownership, including object {object}.")]
    CircularObjectOwnership { object: ObjectID },

    //
    // Coin errors
    //
    #[error("Insufficient coin balance for operation.")]
    InsufficientCoinBalance,
    #[error("The coin balance overflows u64")]
    CoinBalanceOverflow,

    //
    // Publish/Upgrade errors
    //
    #[error(
        "Publish Error, Non-zero Address. \
        The modules in the package must have their self-addresses set to zero."
    )]
    PublishErrorNonZeroAddress,

    #[error(
        "Sui Move Bytecode Verification Error. \
        Please run the Sui Move Verifier for more information."
    )]
    SuiMoveVerificationError,

    //
    // Errors from the Move VM
    //
    // Indicates an error from a non-abort instruction
    #[error(
        "Move Primitive Runtime Error. Location: {0}. \
        Arithmetic error, stack overflow, max value depth, etc."
    )]
    MovePrimitiveRuntimeError(MoveLocationOpt),
    #[error("Move Runtime Abort. Location: {0}, Abort Code: {1}")]
    MoveAbort(MoveLocation, u64),
    #[error(
        "Move Bytecode Verification Error. \
        Please run the Bytecode Verifier for more information."
    )]
    VMVerificationOrDeserializationError,
    #[error("MOVE VM INVARIANT VIOLATION.")]
    VMInvariantViolation,

    //
    // Programmable Transaction Errors
    //
    #[error("Function Not Found.")]
    FunctionNotFound,
    #[error(
        "Arity mismatch for Move function. \
        The number of arguments does not match the number of parameters"
    )]
    ArityMismatch,
    #[error(
        "Type arity mismatch for Move function. \
        Mismatch between the number of actual versus expected type arguments."
    )]
    TypeArityMismatch,
    #[error("Non Entry Function Invoked. Move Call must start with an entry function")]
    NonEntryFunctionInvoked,
    #[error("Invalid command argument at {arg_idx}. {kind}")]
    CommandArgumentError {
        arg_idx: u16,
        kind: CommandArgumentError,
    },
    #[error("Error for type argument at index {argument_idx}: {kind}")]
    TypeArgumentError {
        argument_idx: TypeParameterIndex,
        kind: TypeArgumentError,
    },
    #[error(
        "Unused result without the drop ability. \
        Command result {result_idx}, return value {secondary_idx}"
    )]
    UnusedValueWithoutDrop { result_idx: u16, secondary_idx: u16 },
    #[error(
        "Invalid public Move function signature. \
        Unsupported return type for return value {idx}"
    )]
    InvalidPublicFunctionReturnType { idx: u16 },
    #[error("Invalid Transfer Object, object does not have public transfer.")]
    InvalidTransferObject,

    //
    // Post-execution errors
    //
    // Indicates the effects from the transaction are too large
    #[error(
        "Effects of size {current_size} bytes too large. \
    Limit is {max_size} bytes"
    )]
    EffectsTooLarge { current_size: u64, max_size: u64 },

    #[error(
        "Publish/Upgrade Error, Missing dependency. \
         A dependency of a published or upgraded package has not been assigned an on-chain \
         address."
    )]
    PublishUpgradeMissingDependency,

    #[error(
        "Publish/Upgrade Error, Dependency downgrade. \
         Indirect (transitive) dependency of published or upgraded package has been assigned an \
         on-chain version that is less than the version required by one of the package's \
         transitive dependencies."
    )]
    PublishUpgradeDependencyDowngrade,

    #[error("Invalid package upgrade. {upgrade_error}")]
    PackageUpgradeError { upgrade_error: PackageUpgradeError },

    // Indicates the transaction tried to write objects too large to storage
    #[error(
        "Written objects of {current_size} bytes too large. \
    Limit is {max_size} bytes"
    )]
    WrittenObjectsTooLarge { current_size: u64, max_size: u64 },
    // NOTE: if you want to add a new enum,
    // please add it at the end for Rust SDK backward compatibility.
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Hash)]
pub struct MoveLocation {
    pub module: ModuleId,
    pub function: u16,
    pub instruction: CodeOffset,
    pub function_name: Option<String>,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Hash)]
pub struct MoveLocationOpt(pub Option<MoveLocation>);

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Hash, Error)]
pub enum CommandArgumentError {
    #[error("The type of the value does not match the expected type")]
    TypeMismatch,
    #[error("The argument cannot be deserialized into a value of the specified type")]
    InvalidBCSBytes,
    #[error("The argument cannot be instantiated from raw bytes")]
    InvalidUsageOfPureArg,
    #[error(
        "Invalid argument to private entry function. \
        These functions cannot take arguments from other Move functions"
    )]
    InvalidArgumentToPrivateEntryFunction,
    #[error("Out of bounds access to input or result vector {idx}")]
    IndexOutOfBounds { idx: u16 },
    #[error(
        "Out of bounds secondary access to result vector \
        {result_idx} at secondary index {secondary_idx}"
    )]
    SecondaryIndexOutOfBounds { result_idx: u16, secondary_idx: u16 },
    #[error(
        "Invalid usage of result {result_idx}, \
        expected a single result but found either no return values or multiple."
    )]
    InvalidResultArity { result_idx: u16 },
    #[error(
        "Invalid taking of the Gas coin. \
        It can only be used by-value with TransferObjects"
    )]
    InvalidGasCoinUsage,
    #[error(
        "Invalid usage of value. \
        Mutably borrowed values require unique usage. \
        Immutably borrowed values cannot be taken or borrowed mutably. \
        Taken values cannot be used again."
    )]
    InvalidValueUsage,
    #[error("Immutable and shared objects cannot be passed by-value.")]
    InvalidObjectByValue,
    #[error("Immutable objects cannot be passed by mutable reference, &mut.")]
    InvalidObjectByMutRef,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Hash, Error)]
pub enum PackageUpgradeError {
    #[error("Unable to fetch package at {package_id}")]
    UnableToFetchPackage { package_id: ObjectID },
    #[error("Object {object_id} is not a package")]
    NotAPackage { object_id: ObjectID },
    #[error("New package is incompatible with previous version")]
    IncompatibleUpgrade,
    #[error("Digest in upgrade ticket and computed digest disagree")]
    DigestDoesNotMatch { digest: Vec<u8> },
    #[error("Upgrade policy {policy} is not a valid upgrade policy")]
    UnknownUpgradePolicy { policy: u8 },
    #[error("Package ID {package_id} does not match package ID in upgrade ticket {ticket_id}")]
    PackageIDDoesNotMatch {
        package_id: ObjectID,
        ticket_id: ObjectID,
    },
}

#[derive(Eq, PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash, Error)]
pub enum TypeArgumentError {
    #[error("A type was not found in the module specified.")]
    TypeNotFound,
    #[error("A type provided did not match the specified constraints.")]
    ConstraintNotSatisfied,
}

impl ExecutionFailureStatus {
    pub fn command_argument_error(kind: CommandArgumentError, arg_idx: u16) -> Self {
        Self::CommandArgumentError { arg_idx, kind }
    }
}

impl Display for MoveLocationOpt {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            None => write!(f, "UNKNOWN"),
            Some(l) => write!(f, "{l}"),
        }
    }
}

impl Display for MoveLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self {
            module,
            function,
            instruction,
            function_name,
        } = self;
        if let Some(fname) = function_name {
            write!(
                f,
                "{module}::{fname} (function index {function}) at offset {instruction}"
            )
        } else {
            write!(
                f,
                "{module} in function definition {function} at offset {instruction}"
            )
        }
    }
}

impl ExecutionStatus {
    pub fn new_failure(
        error: ExecutionFailureStatus,
        command: Option<CommandIndex>,
    ) -> ExecutionStatus {
        ExecutionStatus::Failure { error, command }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, ExecutionStatus::Success { .. })
    }

    pub fn is_err(&self) -> bool {
        matches!(self, ExecutionStatus::Failure { .. })
    }

    pub fn unwrap(&self) {
        match self {
            ExecutionStatus::Success => {}
            ExecutionStatus::Failure { .. } => {
                panic!("Unable to unwrap() on {:?}", self);
            }
        }
    }

    pub fn unwrap_err(self) -> (ExecutionFailureStatus, Option<CommandIndex>) {
        match self {
            ExecutionStatus::Success { .. } => {
                panic!("Unable to unwrap() on {:?}", self);
            }
            ExecutionStatus::Failure { error, command } => (error, command),
        }
    }
}
