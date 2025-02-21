use super::TryFromProtoError;
use tap::Pipe;

//
// ExecutionStatus
//

impl From<sui_sdk_types::ExecutionStatus> for super::ExecutionStatus {
    fn from(value: sui_sdk_types::ExecutionStatus) -> Self {
        match value {
            sui_sdk_types::ExecutionStatus::Success => Self {
                success: Some(true),
                status: None,
            },
            sui_sdk_types::ExecutionStatus::Failure { error, command } => Self {
                success: Some(false),
                status: Some(super::FailureStatus {
                    command,
                    execution_error: Some(error.into()),
                }),
            },
        }
    }
}

impl TryFrom<&super::ExecutionStatus> for sui_sdk_types::ExecutionStatus {
    type Error = TryFromProtoError;

    fn try_from(value: &super::ExecutionStatus) -> Result<Self, Self::Error> {
        let success = value
            .success
            .ok_or_else(|| TryFromProtoError::missing("success"))?;
        match (success, &value.status) {
            (true, None) => Self::Success,
            (
                false,
                Some(super::FailureStatus {
                    command,
                    execution_error,
                }),
            ) => Self::Failure {
                error: execution_error
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("execution_error"))?
                    .try_into()?,
                command: *command,
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

impl From<sui_sdk_types::ExecutionError> for super::failure_status::ExecutionError {
    fn from(value: sui_sdk_types::ExecutionError) -> Self {
        use sui_sdk_types::ExecutionError::*;
        match value {
            InsufficientGas => Self::InsufficientGas(()),
            InvalidGasObject => Self::InvalidGasObject(()),
            InvariantViolation => Self::InvariantViolation(()),
            FeatureNotYetSupported => Self::FeatureNotYetSupported(()),
            ObjectTooBig {
                object_size,
                max_object_size,
            } => Self::ObjectTooBig(super::SizeError {
                size: Some(object_size),
                max_size: Some(max_object_size),
            }),
            PackageTooBig {
                object_size,
                max_object_size,
            } => Self::PackageTooBig(super::SizeError {
                size: Some(object_size),
                max_size: Some(max_object_size),
            }),
            CircularObjectOwnership { object } => Self::CircularObjectOwnership(object.into()),
            InsufficientCoinBalance => Self::InsufficientCoinBalance(()),
            CoinBalanceOverflow => Self::CoinBalanceOverflow(()),
            PublishErrorNonZeroAddress => Self::PublishErrorNonZeroAddress(()),
            SuiMoveVerificationError => Self::SuiMoveVerificationError(()),
            MovePrimitiveRuntimeError { location } => {
                Self::MovePrimitiveRuntimeError(super::MoveError {
                    location: location.map(Into::into),
                    abort_code: None,
                })
            }
            MoveAbort { location, code } => Self::MoveAbort(super::MoveError {
                location: Some(location.into()),
                abort_code: Some(code),
            }),
            VmVerificationOrDeserializationError => Self::VmVerificationOrDeserializationError(()),
            VmInvariantViolation => Self::VmInvariantViolation(()),
            FunctionNotFound => Self::FunctionNotFound(()),
            ArityMismatch => Self::ArityMismatch(()),
            TypeArityMismatch => Self::TypeArityMismatch(()),
            NonEntryFunctionInvoked => Self::NonEntryFunctionInvoked(()),
            CommandArgumentError { argument, kind } => {
                Self::CommandArgumentError(super::CommandArgumentError {
                    argument: Some(argument.into()),
                    kind: Some(kind.into()),
                })
            }
            TypeArgumentError {
                type_argument,
                kind,
            } => Self::TypeArgumentError(super::TypeArgumentError {
                type_argument: Some(type_argument.into()),
                kind: Some(kind.into()),
            }),
            UnusedValueWithoutDrop { result, subresult } => {
                Self::UnusedValueWithoutDrop(super::NestedResult {
                    result: Some(result.into()),
                    subresult: Some(subresult.into()),
                })
            }
            InvalidPublicFunctionReturnType { index } => {
                Self::InvalidPublicFunctionReturnType(index.into())
            }
            InvalidTransferObject => Self::InvalidTransferObject(()),
            EffectsTooLarge {
                current_size,
                max_size,
            } => Self::EffectsTooLarge(super::SizeError {
                size: Some(current_size),
                max_size: Some(max_size),
            }),
            PublishUpgradeMissingDependency => Self::PublishUpgradeMissingDependency(()),
            PublishUpgradeDependencyDowngrade => Self::PublishUpgradeDependencyDowngrade(()),
            PackageUpgradeError { kind } => Self::PackageUpgradeError(super::PackageUpgradeError {
                kind: Some(kind.into()),
            }),
            WrittenObjectsTooLarge {
                object_size,
                max_object_size,
            } => Self::WrittenObjectsTooLarge(super::SizeError {
                size: Some(object_size),
                max_size: Some(max_object_size),
            }),
            CertificateDenied => Self::CertificateDenied(()),
            SuiMoveVerificationTimedout => Self::SuiMoveVerificationTimedout(()),
            SharedObjectOperationNotAllowed => Self::SharedObjectOperationNotAllowed(()),
            InputObjectDeleted => Self::InputObjectDeleted(()),
            ExecutionCancelledDueToSharedObjectCongestion { congested_objects } => {
                Self::ExecutionCancelledDueToSharedObjectCongestion(super::CongestedObjectsError {
                    congested_objects: congested_objects.into_iter().map(Into::into).collect(),
                })
            }
            AddressDeniedForCoin { address, coin_type } => {
                Self::AddressDeniedForCoin(super::AddressDeniedForCoinError {
                    coin_type: Some(coin_type),
                    address: Some(address.into()),
                })
            }
            CoinTypeGlobalPause { coin_type } => Self::CoinTypeGlobalPause(coin_type),
            ExecutionCancelledDueToRandomnessUnavailable => {
                Self::ExecutionCancelledDueToRandomnessUnavailable(())
            }
        }
    }
}

impl TryFrom<&super::failure_status::ExecutionError> for sui_sdk_types::ExecutionError {
    type Error = TryFromProtoError;

    fn try_from(value: &super::failure_status::ExecutionError) -> Result<Self, Self::Error> {
        use super::failure_status::ExecutionError::*;

        match value {
            InsufficientGas(()) => Self::InsufficientGas,
            InvalidGasObject(()) => Self::InvalidGasObject,
            InvariantViolation(()) => Self::InvariantViolation,
            FeatureNotYetSupported(()) => Self::FeatureNotYetSupported,
            ObjectTooBig(super::SizeError { size, max_size }) => Self::ObjectTooBig {
                object_size: size.ok_or_else(|| TryFromProtoError::missing("size"))?,
                max_object_size: max_size.ok_or_else(|| TryFromProtoError::missing("max_size"))?,
            },
            PackageTooBig(super::SizeError { size, max_size }) => Self::PackageTooBig {
                object_size: size.ok_or_else(|| TryFromProtoError::missing("size"))?,
                max_object_size: max_size.ok_or_else(|| TryFromProtoError::missing("max_size"))?,
            },
            CircularObjectOwnership(object) => Self::CircularObjectOwnership {
                object: object.try_into()?,
            },
            InsufficientCoinBalance(()) => Self::InsufficientCoinBalance,
            CoinBalanceOverflow(()) => Self::CoinBalanceOverflow,
            PublishErrorNonZeroAddress(()) => Self::PublishErrorNonZeroAddress,
            SuiMoveVerificationError(()) => Self::SuiMoveVerificationError,
            MovePrimitiveRuntimeError(super::MoveError {
                location,
                abort_code: _,
            }) => Self::MovePrimitiveRuntimeError {
                location: location.as_ref().map(TryInto::try_into).transpose()?,
            },
            MoveAbort(super::MoveError {
                location: Some(location),
                abort_code: Some(abort_code),
            }) => Self::MoveAbort {
                location: location.try_into()?,
                code: *abort_code,
            },
            MoveAbort(_) => return Err(TryFromProtoError::missing("location or abort_code")),
            VmVerificationOrDeserializationError(()) => Self::VmVerificationOrDeserializationError,
            VmInvariantViolation(()) => Self::VmInvariantViolation,
            FunctionNotFound(()) => Self::FunctionNotFound,
            ArityMismatch(()) => Self::ArityMismatch,
            TypeArityMismatch(()) => Self::TypeArityMismatch,
            NonEntryFunctionInvoked(()) => Self::NonEntryFunctionInvoked,
            CommandArgumentError(super::CommandArgumentError { argument, kind }) => {
                Self::CommandArgumentError {
                    argument: argument
                        .ok_or_else(|| TryFromProtoError::missing("argument"))?
                        .try_into()?,
                    kind: kind
                        .as_ref()
                        .ok_or_else(|| TryFromProtoError::missing("kind"))?
                        .try_into()?,
                }
            }
            TypeArgumentError(super::TypeArgumentError {
                type_argument,
                kind,
            }) => Self::TypeArgumentError {
                type_argument: type_argument
                    .ok_or_else(|| TryFromProtoError::missing("type_argument"))?
                    .try_into()?,
                kind: kind
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("kind"))?
                    .try_into()?,
            },
            UnusedValueWithoutDrop(super::NestedResult { result, subresult }) => {
                Self::UnusedValueWithoutDrop {
                    result: result
                        .ok_or_else(|| TryFromProtoError::missing("result"))?
                        .try_into()?,
                    subresult: subresult
                        .ok_or_else(|| TryFromProtoError::missing("subresult"))?
                        .try_into()?,
                }
            }
            InvalidPublicFunctionReturnType(index) => Self::InvalidPublicFunctionReturnType {
                index: (*index).try_into()?,
            },
            InvalidTransferObject(()) => Self::InvalidTransferObject,
            EffectsTooLarge(super::SizeError { size, max_size }) => Self::EffectsTooLarge {
                current_size: size.ok_or_else(|| TryFromProtoError::missing("size"))?,
                max_size: max_size.ok_or_else(|| TryFromProtoError::missing("max_size"))?,
            },
            PublishUpgradeMissingDependency(()) => Self::PublishUpgradeMissingDependency,
            PublishUpgradeDependencyDowngrade(()) => Self::PublishUpgradeDependencyDowngrade,
            PackageUpgradeError(super::PackageUpgradeError { kind }) => Self::PackageUpgradeError {
                kind: kind
                    .as_ref()
                    .ok_or_else(|| TryFromProtoError::missing("kind"))?
                    .try_into()?,
            },
            WrittenObjectsTooLarge(super::SizeError { size, max_size }) => {
                Self::WrittenObjectsTooLarge {
                    object_size: size.ok_or_else(|| TryFromProtoError::missing("size"))?,
                    max_object_size: max_size
                        .ok_or_else(|| TryFromProtoError::missing("max_size"))?,
                }
            }
            CertificateDenied(()) => Self::CertificateDenied,
            SuiMoveVerificationTimedout(()) => Self::SuiMoveVerificationTimedout,
            SharedObjectOperationNotAllowed(()) => Self::SharedObjectOperationNotAllowed,
            InputObjectDeleted(()) => Self::InputObjectDeleted,
            ExecutionCancelledDueToSharedObjectCongestion(super::CongestedObjectsError {
                congested_objects,
            }) => Self::ExecutionCancelledDueToSharedObjectCongestion {
                congested_objects: congested_objects
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            },
            AddressDeniedForCoin(super::AddressDeniedForCoinError { address, coin_type }) => {
                Self::AddressDeniedForCoin {
                    address: address
                        .as_ref()
                        .ok_or_else(|| TryFromProtoError::missing("address"))?
                        .try_into()?,
                    coin_type: coin_type
                        .as_ref()
                        .ok_or_else(|| TryFromProtoError::missing("coin_type"))?
                        .to_owned(),
                }
            }
            CoinTypeGlobalPause(coin_type) => Self::CoinTypeGlobalPause {
                coin_type: coin_type.to_owned(),
            },
            ExecutionCancelledDueToRandomnessUnavailable(()) => {
                Self::ExecutionCancelledDueToRandomnessUnavailable
            }
        }
        .pipe(Ok)
    }
}

//
// CommandArgumentError
//

impl From<sui_sdk_types::CommandArgumentError> for super::command_argument_error::Kind {
    fn from(value: sui_sdk_types::CommandArgumentError) -> Self {
        use sui_sdk_types::CommandArgumentError::*;

        match value {
            TypeMismatch => Self::TypeMismatch(()),
            InvalidBcsBytes => Self::InvalidBcsBytes(()),
            InvalidUsageOfPureArgument => Self::InvalidUsageOfPureArgument(()),
            InvalidArgumentToPrivateEntryFunction => {
                Self::InvalidArgumentToPrivateEntryFunction(())
            }
            IndexOutOfBounds { index } => Self::IndexOutOfBounds(index.into()),
            SecondaryIndexOutOfBounds { result, subresult } => {
                Self::SecondaryIndexOutOfBounds(super::NestedResult {
                    result: Some(result.into()),
                    subresult: Some(subresult.into()),
                })
            }
            InvalidResultArity { result } => Self::InvalidResultArity(result.into()),
            InvalidGasCoinUsage => Self::InvalidGasCoinUsage(()),
            InvalidValueUsage => Self::InvalidValueUsage(()),
            InvalidObjectByValue => Self::InvalidObjectByValue(()),
            InvalidObjectByMutRef => Self::InvalidObjectByMutRef(()),
            SharedObjectOperationNotAllowed => Self::SharedObjectOperationNotAllowed(()),
        }
    }
}

impl TryFrom<&super::command_argument_error::Kind> for sui_sdk_types::CommandArgumentError {
    type Error = TryFromProtoError;

    fn try_from(value: &super::command_argument_error::Kind) -> Result<Self, Self::Error> {
        use super::command_argument_error::Kind::*;
        use super::NestedResult;

        match value {
            TypeMismatch(()) => Self::TypeMismatch,
            InvalidBcsBytes(()) => Self::InvalidBcsBytes,
            InvalidUsageOfPureArgument(()) => Self::InvalidUsageOfPureArgument,
            InvalidArgumentToPrivateEntryFunction(()) => {
                Self::InvalidArgumentToPrivateEntryFunction
            }
            IndexOutOfBounds(index) => Self::IndexOutOfBounds {
                index: (*index).try_into()?,
            },
            SecondaryIndexOutOfBounds(NestedResult { result, subresult }) => {
                Self::SecondaryIndexOutOfBounds {
                    result: result
                        .ok_or_else(|| TryFromProtoError::missing("result"))?
                        .try_into()?,
                    subresult: subresult
                        .ok_or_else(|| TryFromProtoError::missing("subresult"))?
                        .try_into()?,
                }
            }
            InvalidResultArity(result) => Self::InvalidResultArity {
                result: (*result).try_into()?,
            },
            InvalidGasCoinUsage(()) => Self::InvalidGasCoinUsage,
            InvalidValueUsage(()) => Self::InvalidValueUsage,
            InvalidObjectByValue(()) => Self::InvalidObjectByValue,
            InvalidObjectByMutRef(()) => Self::InvalidObjectByMutRef,
            SharedObjectOperationNotAllowed(()) => Self::SharedObjectOperationNotAllowed,
        }
        .pipe(Ok)
    }
}

//
// TypeArgumentError
//

impl From<sui_sdk_types::TypeArgumentError> for super::type_argument_error::Kind {
    fn from(value: sui_sdk_types::TypeArgumentError) -> Self {
        use sui_sdk_types::TypeArgumentError::*;

        match value {
            TypeNotFound => Self::TypeNotFound(()),
            ConstraintNotSatisfied => Self::ConstraintNotSatisfied(()),
        }
    }
}

impl TryFrom<&super::type_argument_error::Kind> for sui_sdk_types::TypeArgumentError {
    type Error = TryFromProtoError;

    fn try_from(value: &super::type_argument_error::Kind) -> Result<Self, Self::Error> {
        use super::type_argument_error::Kind::*;

        match value {
            TypeNotFound(()) => Self::TypeNotFound,
            ConstraintNotSatisfied(()) => Self::ConstraintNotSatisfied,
        }
        .pipe(Ok)
    }
}

//
// PackageUpgradeError
//

impl From<sui_sdk_types::PackageUpgradeError> for super::package_upgrade_error::Kind {
    fn from(value: sui_sdk_types::PackageUpgradeError) -> Self {
        use sui_sdk_types::PackageUpgradeError::*;

        match value {
            UnableToFetchPackage { package_id } => Self::UnableToFetchPackage(package_id.into()),
            NotAPackage { object_id } => Self::NotAPackage(object_id.into()),
            IncompatibleUpgrade => Self::IncompatibleUpgrade(()),
            DigestDoesNotMatch { digest } => Self::DigetsDoesNotMatch(digest.into()),
            UnknownUpgradePolicy { policy } => Self::UnknownUpgradePolicy(policy.into()),
            PackageIdDoesNotMatch {
                package_id,
                ticket_id,
            } => Self::PackageIdDoesNotMatch(super::PackageIdDoesNotMatch {
                package_id: Some(package_id.into()),
                ticket_id: Some(ticket_id.into()),
            }),
        }
    }
}

impl TryFrom<&super::package_upgrade_error::Kind> for sui_sdk_types::PackageUpgradeError {
    type Error = TryFromProtoError;

    fn try_from(value: &super::package_upgrade_error::Kind) -> Result<Self, Self::Error> {
        use super::package_upgrade_error::Kind::*;

        match value {
            UnableToFetchPackage(package_id) => Self::UnableToFetchPackage {
                package_id: package_id.try_into()?,
            },
            NotAPackage(object_id) => Self::NotAPackage {
                object_id: object_id.try_into()?,
            },
            IncompatibleUpgrade(()) => Self::IncompatibleUpgrade,
            DigetsDoesNotMatch(digest) => Self::DigestDoesNotMatch {
                digest: digest.try_into()?,
            },
            UnknownUpgradePolicy(policy) => Self::UnknownUpgradePolicy {
                policy: (*policy).try_into()?,
            },
            PackageIdDoesNotMatch(super::PackageIdDoesNotMatch {
                package_id: Some(package_id),
                ticket_id: Some(ticket_id),
            }) => Self::PackageIdDoesNotMatch {
                package_id: package_id.try_into()?,
                ticket_id: ticket_id.try_into()?,
            },
            PackageIdDoesNotMatch(_) => {
                return Err(TryFromProtoError::missing(
                    "missing package_id or ticket_id",
                ))
            }
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
            package: Some(value.package.into()),
            module: Some(value.module.into()),
            function: Some(value.function.into()),
            instruction: Some(value.instruction.into()),
            function_name: value.function_name.map(Into::into),
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
            .try_into()?;
        let module = value
            .module
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("module"))?
            .try_into()?;
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
            .map(TryFrom::try_from)
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
