// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::TryFromProtoError;
use tap::Pipe;

//
// ExecutionStatus
//

impl From<sui_sdk_types::ExecutionStatus> for super::ExecutionStatus {
    fn from(value: sui_sdk_types::ExecutionStatus) -> Self {
        match value {
            sui_sdk_types::ExecutionStatus::Success => Self {
                success: Some(true),
                error: None,
            },
            sui_sdk_types::ExecutionStatus::Failure { error, command } => {
                let mut error = super::ExecutionError::from(error);
                error.command = command;
                Self {
                    success: Some(false),
                    error: Some(error),
                }
            }
        }
    }
}

impl TryFrom<&super::ExecutionStatus> for sui_sdk_types::ExecutionStatus {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ExecutionStatus) -> Result<Self, Self::Error> {
        let success = value
            .success
            .ok_or_else(|| TryFromProtoError::missing("success"))?;
        match (success, &value.error) {
            (true, None) => Self::Success,
            (false, Some(error)) => Self::Failure {
                error: error.try_into()?,
                command: error.command,
            },
            (true, Some(_)) | (false, None) => {
                return Err(TryFromProtoError::from_error("invalid execution status"))
            }
        }
        .pipe(Ok)
    }
}

//
// ExecutionError
//

impl From<sui_sdk_types::ExecutionError> for super::ExecutionError {
    fn from(value: sui_sdk_types::ExecutionError) -> Self {
        use super::execution_error::ExecutionErrorKind;
        use sui_sdk_types::ExecutionError::*;

        let mut message = Self::default();

        let kind = match value {
            InsufficientGas => ExecutionErrorKind::InsufficientGas,
            InvalidGasObject => ExecutionErrorKind::InvalidGasObject,
            InvariantViolation => ExecutionErrorKind::InvariantViolation,
            FeatureNotYetSupported => ExecutionErrorKind::FeatureNotYetSupported,
            ObjectTooBig {
                object_size,
                max_object_size,
            } => {
                message.size_error = Some(super::SizeError {
                    size: Some(object_size),
                    max_size: Some(max_object_size),
                });
                ExecutionErrorKind::ObjectTooBig
            }
            PackageTooBig {
                object_size,
                max_object_size,
            } => {
                message.size_error = Some(super::SizeError {
                    size: Some(object_size),
                    max_size: Some(max_object_size),
                });
                ExecutionErrorKind::PackageTooBig
            }
            CircularObjectOwnership { object } => {
                message.object_id = Some(object.to_string());
                ExecutionErrorKind::CircularObjectOwnership
            }
            InsufficientCoinBalance => ExecutionErrorKind::InsufficientCoinBalance,
            CoinBalanceOverflow => ExecutionErrorKind::CoinBalanceOverflow,
            PublishErrorNonZeroAddress => ExecutionErrorKind::PublishErrorNonZeroAddress,
            SuiMoveVerificationError => ExecutionErrorKind::SuiMoveVerificationError,
            MovePrimitiveRuntimeError { location } => {
                message.location = location.map(Into::into);
                ExecutionErrorKind::MovePrimitiveRuntimeError
            }
            MoveAbort { location, code } => {
                message.location = Some(location.into());
                message.abort_code = Some(code);
                ExecutionErrorKind::MoveAbort
            }
            VmVerificationOrDeserializationError => {
                ExecutionErrorKind::VmVerificationOrDeserializationError
            }
            VmInvariantViolation => ExecutionErrorKind::VmInvariantViolation,
            FunctionNotFound => ExecutionErrorKind::FunctionNotFound,
            ArityMismatch => ExecutionErrorKind::ArityMismatch,
            TypeArityMismatch => ExecutionErrorKind::TypeArityMismatch,
            NonEntryFunctionInvoked => ExecutionErrorKind::NonEntryFunctionInvoked,
            CommandArgumentError { argument, kind } => {
                let mut command_argument_error = super::CommandArgumentError::from(kind);
                command_argument_error.argument = Some(argument.into());
                message.command_argument_error = Some(command_argument_error);
                ExecutionErrorKind::CommandArgumentError
            }
            TypeArgumentError {
                type_argument,
                kind,
            } => {
                message.type_argument_error = Some(super::TypeArgumentError {
                    type_argument: Some(type_argument.into()),
                    kind: Some(
                        super::type_argument_error::TypeArgumentErrorKind::from(kind).into(),
                    ),
                });
                ExecutionErrorKind::TypeArgumentError
            }
            UnusedValueWithoutDrop { result, subresult } => {
                message.index = Some(result.into());
                message.subresult = Some(subresult.into());
                ExecutionErrorKind::UnusedValueWithoutDrop
            }
            InvalidPublicFunctionReturnType { index } => {
                message.index = Some(index.into());
                ExecutionErrorKind::InvalidPublicFunctionReturnType
            }
            InvalidTransferObject => ExecutionErrorKind::InvalidTransferObject,
            EffectsTooLarge {
                current_size,
                max_size,
            } => {
                message.size_error = Some(super::SizeError {
                    size: Some(current_size),
                    max_size: Some(max_size),
                });
                ExecutionErrorKind::EffectsTooLarge
            }
            PublishUpgradeMissingDependency => ExecutionErrorKind::PublishUpgradeMissingDependency,
            PublishUpgradeDependencyDowngrade => {
                ExecutionErrorKind::PublishUpgradeDependencyDowngrade
            }
            PackageUpgradeError { kind } => {
                message.package_upgrade_error = Some(kind.into());
                ExecutionErrorKind::PackageUpgradeError
            }
            WrittenObjectsTooLarge {
                object_size,
                max_object_size,
            } => {
                message.size_error = Some(super::SizeError {
                    size: Some(object_size),
                    max_size: Some(max_object_size),
                });

                ExecutionErrorKind::WrittenObjectsTooLarge
            }
            CertificateDenied => ExecutionErrorKind::CertificateDenied,
            SuiMoveVerificationTimedout => ExecutionErrorKind::SuiMoveVerificationTimedout,
            SharedObjectOperationNotAllowed => ExecutionErrorKind::SharedObjectOperationNotAllowed,
            InputObjectDeleted => ExecutionErrorKind::InputObjectDeleted,
            ExecutionCanceledDueToSharedObjectCongestion { congested_objects } => {
                message.congested_objects =
                    congested_objects.iter().map(ToString::to_string).collect();

                ExecutionErrorKind::ExecutionCanceledDueToSharedObjectCongestion
            }
            AddressDeniedForCoin { address, coin_type } => {
                message.coin_type = Some(coin_type);
                message.address = Some(address.to_string());
                ExecutionErrorKind::AddressDeniedForCoin
            }
            CoinTypeGlobalPause { coin_type } => {
                message.coin_type = Some(coin_type);
                ExecutionErrorKind::CoinTypeGlobalPause
            }
            ExecutionCanceledDueToRandomnessUnavailable => {
                ExecutionErrorKind::ExecutionCanceledDueToRandomnessUnavailable
            }
        };

        message.set_kind(kind);
        message
    }
}

impl TryFrom<&super::ExecutionError> for sui_sdk_types::ExecutionError {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ExecutionError) -> Result<Self, Self::Error> {
        use super::execution_error::ExecutionErrorKind::*;

        match value.kind() {
            Unknown => return Err(TryFromProtoError::from_error("unknown ExecutionErrorKind")),

            InsufficientGas => Self::InsufficientGas,
            InvalidGasObject => Self::InvalidGasObject,
            InvariantViolation => Self::InvariantViolation,
            FeatureNotYetSupported => Self::FeatureNotYetSupported,
            ObjectTooBig => {
                let super::SizeError { size, max_size } = value
                    .size_error
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("size_error"))?;
                Self::ObjectTooBig {
                    object_size: size.ok_or_else(|| TryFromProtoError::missing("size"))?,
                    max_object_size: max_size
                        .ok_or_else(|| TryFromProtoError::missing("max_size"))?,
                }
            }
            PackageTooBig => {
                let super::SizeError { size, max_size } = value
                    .size_error
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("size_error"))?;
                Self::PackageTooBig {
                    object_size: size.ok_or_else(|| TryFromProtoError::missing("size"))?,
                    max_object_size: max_size
                        .ok_or_else(|| TryFromProtoError::missing("max_size"))?,
                }
            }
            CircularObjectOwnership => Self::CircularObjectOwnership {
                object: value
                    .object_id
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("object_id"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
            },
            InsufficientCoinBalance => Self::InsufficientCoinBalance,
            CoinBalanceOverflow => Self::CoinBalanceOverflow,
            PublishErrorNonZeroAddress => Self::PublishErrorNonZeroAddress,
            SuiMoveVerificationError => Self::SuiMoveVerificationError,
            MovePrimitiveRuntimeError => Self::MovePrimitiveRuntimeError {
                location: value.location.as_ref().map(TryInto::try_into).transpose()?,
            },
            MoveAbort => Self::MoveAbort {
                location: value
                    .location
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("location"))?
                    .try_into()?,
                code: value
                    .abort_code
                    .ok_or_else(|| TryFromProtoError::missing("abort_code"))?,
            },
            VmVerificationOrDeserializationError => Self::VmVerificationOrDeserializationError,
            VmInvariantViolation => Self::VmInvariantViolation,
            FunctionNotFound => Self::FunctionNotFound,
            ArityMismatch => Self::ArityMismatch,
            TypeArityMismatch => Self::TypeArityMismatch,
            NonEntryFunctionInvoked => Self::NonEntryFunctionInvoked,
            CommandArgumentError => {
                let command_argument_error = value
                    .command_argument_error
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("command_argument_error"))?;
                Self::CommandArgumentError {
                    argument: command_argument_error
                        .argument
                        .ok_or_else(|| TryFromProtoError::missing("argument"))?
                        .try_into()?,
                    kind: command_argument_error.try_into()?,
                }
            }
            TypeArgumentError => {
                let type_argument_error = value
                    .type_argument_error
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("type_argument_error"))?;
                Self::TypeArgumentError {
                    type_argument: type_argument_error
                        .type_argument
                        .ok_or_else(|| TryFromProtoError::missing("type_argument"))?
                        .try_into()?,
                    kind: type_argument_error.kind().try_into()?,
                }
            }
            UnusedValueWithoutDrop => Self::UnusedValueWithoutDrop {
                result: value
                    .index
                    .ok_or_else(|| TryFromProtoError::missing("result"))?
                    .try_into()?,
                subresult: value
                    .subresult
                    .ok_or_else(|| TryFromProtoError::missing("subresult"))?
                    .try_into()?,
            },
            InvalidPublicFunctionReturnType => Self::InvalidPublicFunctionReturnType {
                index: value
                    .index
                    .ok_or_else(|| TryFromProtoError::missing("index"))?
                    .try_into()?,
            },
            InvalidTransferObject => Self::InvalidTransferObject,
            EffectsTooLarge => {
                let super::SizeError { size, max_size } = value
                    .size_error
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("size_error"))?;
                Self::EffectsTooLarge {
                    current_size: size.ok_or_else(|| TryFromProtoError::missing("size"))?,
                    max_size: max_size.ok_or_else(|| TryFromProtoError::missing("max_size"))?,
                }
            }
            PublishUpgradeMissingDependency => Self::PublishUpgradeMissingDependency,
            PublishUpgradeDependencyDowngrade => Self::PublishUpgradeDependencyDowngrade,
            PackageUpgradeError => Self::PackageUpgradeError {
                kind: value
                    .package_upgrade_error
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("package_upgrade_error"))?
                    .try_into()?,
            },
            WrittenObjectsTooLarge => {
                let super::SizeError { size, max_size } = value
                    .size_error
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("size_error"))?;

                Self::WrittenObjectsTooLarge {
                    object_size: size.ok_or_else(|| TryFromProtoError::missing("size"))?,
                    max_object_size: max_size
                        .ok_or_else(|| TryFromProtoError::missing("max_size"))?,
                }
            }
            CertificateDenied => Self::CertificateDenied,
            SuiMoveVerificationTimedout => Self::SuiMoveVerificationTimedout,
            SharedObjectOperationNotAllowed => Self::SharedObjectOperationNotAllowed,
            InputObjectDeleted => Self::InputObjectDeleted,
            ExecutionCanceledDueToSharedObjectCongestion => {
                Self::ExecutionCanceledDueToSharedObjectCongestion {
                    congested_objects: value
                        .congested_objects
                        .iter()
                        .map(|s| s.parse())
                        .collect::<Result<_, _>>()
                        .map_err(TryFromProtoError::from_error)?,
                }
            }
            AddressDeniedForCoin => Self::AddressDeniedForCoin {
                address: value
                    .address
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("address"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
                coin_type: value
                    .coin_type
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("coin_type"))?
                    .to_owned(),
            },
            CoinTypeGlobalPause => Self::CoinTypeGlobalPause {
                coin_type: value
                    .coin_type
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("coin_type"))?
                    .to_owned(),
            },
            ExecutionCanceledDueToRandomnessUnavailable => {
                Self::ExecutionCanceledDueToRandomnessUnavailable
            }
        }
        .pipe(Ok)
    }
}

//
// CommandArgumentError
//

impl From<sui_sdk_types::CommandArgumentError> for super::CommandArgumentError {
    fn from(value: sui_sdk_types::CommandArgumentError) -> Self {
        use super::command_argument_error::CommandArgumentErrorKind;
        use sui_sdk_types::CommandArgumentError::*;

        let mut message = Self::default();

        let kind = match value {
            TypeMismatch => CommandArgumentErrorKind::TypeMismatch,
            InvalidBcsBytes => CommandArgumentErrorKind::InvalidBcsBytes,
            InvalidUsageOfPureArgument => CommandArgumentErrorKind::InvalidUsageOfPureArgument,
            InvalidArgumentToPrivateEntryFunction => {
                CommandArgumentErrorKind::InvalidArgumentToPrivateEntryFunction
            }
            IndexOutOfBounds { index } => {
                message.index = Some(index.into());
                CommandArgumentErrorKind::IndexOutOfBounds
            }
            SecondaryIndexOutOfBounds { result, subresult } => {
                message.index = Some(result.into());
                message.subresult = Some(subresult.into());
                CommandArgumentErrorKind::SecondaryIndexOutOfBounds
            }
            InvalidResultArity { result } => {
                message.index = Some(result.into());
                CommandArgumentErrorKind::InvalidResultArity
            }
            InvalidGasCoinUsage => CommandArgumentErrorKind::InvalidGasCoinUsage,
            InvalidValueUsage => CommandArgumentErrorKind::InvalidValueUsage,
            InvalidObjectByValue => CommandArgumentErrorKind::InvalidObjectByValue,
            InvalidObjectByMutRef => CommandArgumentErrorKind::InvalidObjectByMutRef,
            SharedObjectOperationNotAllowed => {
                CommandArgumentErrorKind::SharedObjectOperationNotAllowed
            }
        };

        message.set_kind(kind);
        message
    }
}

impl TryFrom<&super::CommandArgumentError> for sui_sdk_types::CommandArgumentError {
    type Error = TryFromProtoError;

    fn try_from(value: &super::CommandArgumentError) -> Result<Self, Self::Error> {
        use super::command_argument_error::CommandArgumentErrorKind::*;

        match value.kind() {
            Unknown => {
                return Err(TryFromProtoError::from_error(
                    "unknown CommandArgumentErrorKind",
                ))
            }
            TypeMismatch => Self::TypeMismatch,
            InvalidBcsBytes => Self::InvalidBcsBytes,
            InvalidUsageOfPureArgument => Self::InvalidUsageOfPureArgument,
            InvalidArgumentToPrivateEntryFunction => Self::InvalidArgumentToPrivateEntryFunction,
            IndexOutOfBounds => Self::IndexOutOfBounds {
                index: value
                    .index
                    .ok_or_else(|| TryFromProtoError::missing("index"))?
                    .try_into()?,
            },
            SecondaryIndexOutOfBounds => Self::SecondaryIndexOutOfBounds {
                result: value
                    .index
                    .ok_or_else(|| TryFromProtoError::missing("index"))?
                    .try_into()?,
                subresult: value
                    .subresult
                    .ok_or_else(|| TryFromProtoError::missing("subresult"))?
                    .try_into()?,
            },
            InvalidResultArity => Self::InvalidResultArity {
                result: value
                    .index
                    .ok_or_else(|| TryFromProtoError::missing("index"))?
                    .try_into()?,
            },
            InvalidGasCoinUsage => Self::InvalidGasCoinUsage,
            InvalidValueUsage => Self::InvalidValueUsage,
            InvalidObjectByValue => Self::InvalidObjectByValue,
            InvalidObjectByMutRef => Self::InvalidObjectByMutRef,
            SharedObjectOperationNotAllowed => Self::SharedObjectOperationNotAllowed,
        }
        .pipe(Ok)
    }
}

//
// TypeArgumentError
//

impl From<sui_sdk_types::TypeArgumentError> for super::type_argument_error::TypeArgumentErrorKind {
    fn from(value: sui_sdk_types::TypeArgumentError) -> Self {
        use sui_sdk_types::TypeArgumentError::*;

        match value {
            TypeNotFound => Self::TypeNotFound,
            ConstraintNotSatisfied => Self::ConstraintNotSatisfied,
        }
    }
}

impl TryFrom<super::type_argument_error::TypeArgumentErrorKind>
    for sui_sdk_types::TypeArgumentError
{
    type Error = TryFromProtoError;

    fn try_from(
        value: super::type_argument_error::TypeArgumentErrorKind,
    ) -> Result<Self, Self::Error> {
        use super::type_argument_error::TypeArgumentErrorKind::*;

        match value {
            Unknown => {
                return Err(TryFromProtoError::from_error(
                    "unknown TypeArgumentErrorKind",
                ))
            }
            TypeNotFound => Self::TypeNotFound,
            ConstraintNotSatisfied => Self::ConstraintNotSatisfied,
        }
        .pipe(Ok)
    }
}

//
// PackageUpgradeError
//

impl From<sui_sdk_types::PackageUpgradeError> for super::PackageUpgradeError {
    fn from(value: sui_sdk_types::PackageUpgradeError) -> Self {
        use super::package_upgrade_error::PackageUpgradeErrorKind;
        use sui_sdk_types::PackageUpgradeError::*;

        let mut message = Self::default();

        let kind = match value {
            UnableToFetchPackage { package_id } => {
                message.package_id = Some(package_id.to_string());
                PackageUpgradeErrorKind::UnableToFetchPackage
            }
            NotAPackage { object_id } => {
                message.package_id = Some(object_id.to_string());
                PackageUpgradeErrorKind::NotAPackage
            }
            IncompatibleUpgrade => PackageUpgradeErrorKind::IncompatibleUpgrade,
            DigestDoesNotMatch { digest } => {
                message.digest = Some(digest.to_string());
                PackageUpgradeErrorKind::DigetsDoesNotMatch
            }
            UnknownUpgradePolicy { policy } => {
                message.policy = Some(policy.into());
                PackageUpgradeErrorKind::UnknownUpgradePolicy
            }
            PackageIdDoesNotMatch {
                package_id,
                ticket_id,
            } => {
                message.package_id = Some(package_id.to_string());
                message.ticket_id = Some(ticket_id.to_string());
                PackageUpgradeErrorKind::PackageIdDoesNotMatch
            }
        };

        message.set_kind(kind);
        message
    }
}

impl TryFrom<&super::PackageUpgradeError> for sui_sdk_types::PackageUpgradeError {
    type Error = TryFromProtoError;

    fn try_from(value: &super::PackageUpgradeError) -> Result<Self, Self::Error> {
        use super::package_upgrade_error::PackageUpgradeErrorKind::*;

        match value.kind() {
            Unknown => {
                return Err(TryFromProtoError::from_error(
                    "unknown PackageUpgradeErrorKind",
                ))
            }
            UnableToFetchPackage => Self::UnableToFetchPackage {
                package_id: value
                    .package_id
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("package_id"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
            },
            NotAPackage => Self::NotAPackage {
                object_id: value
                    .package_id
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("package_id"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
            },
            IncompatibleUpgrade => Self::IncompatibleUpgrade,
            DigetsDoesNotMatch => Self::DigestDoesNotMatch {
                digest: value
                    .digest
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("digest"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
            },
            UnknownUpgradePolicy => Self::UnknownUpgradePolicy {
                policy: value
                    .policy
                    .ok_or_else(|| TryFromProtoError::missing("policy"))?
                    .try_into()?,
            },
            PackageIdDoesNotMatch => Self::PackageIdDoesNotMatch {
                package_id: value
                    .package_id
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("package_id"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
                ticket_id: value
                    .ticket_id
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("ticket_id"))?
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
            },
        }
        .pipe(Ok)
    }
}

//
// MoveLocation
//

impl From<sui_sdk_types::MoveLocation> for super::MoveLocation {
    fn from(value: sui_sdk_types::MoveLocation) -> Self {
        Self {
            package: Some(value.package.to_string()),
            module: Some(value.module.to_string()),
            function: Some(value.function.into()),
            instruction: Some(value.instruction.into()),
            function_name: value.function_name.map(|name| name.to_string()),
        }
    }
}

impl TryFrom<&super::MoveLocation> for sui_sdk_types::MoveLocation {
    type Error = TryFromProtoError;

    fn try_from(value: &super::MoveLocation) -> Result<Self, Self::Error> {
        let package = value
            .package
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("package"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;
        let module = value
            .module
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("module"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;
        let function = value
            .function
            .ok_or_else(|| TryFromProtoError::missing("function"))?
            .try_into()?;
        let instruction = value
            .instruction
            .ok_or_else(|| TryFromProtoError::missing("instruction"))?
            .try_into()?;
        let function_name = value
            .function_name
            .as_ref()
            .map(|name| name.parse().map_err(TryFromProtoError::from_error))
            .transpose()?;

        Ok(Self {
            package,
            module,
            function,
            instruction,
            function_name,
        })
    }
}
