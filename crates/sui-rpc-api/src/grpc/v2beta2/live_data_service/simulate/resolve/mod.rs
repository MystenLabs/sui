// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::rc::Rc;

use crate::error::ObjectNotFoundError;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::rpc::v2beta2::Transaction;
use crate::reader::StateReader;
use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use bytes::Bytes;
use move_binary_format::normalized;
use sui_protocol_config::ProtocolConfig;
use sui_sdk_types::Argument;
use sui_sdk_types::Command;
use sui_sdk_types::ObjectId;
use sui_types::base_types::ObjectRef;
use sui_types::move_package::MovePackage;
use sui_types::transaction::CallArg;
use sui_types::transaction::GasData;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::ProgrammableTransaction;
use sui_types::transaction::TransactionData;
use tap::Pipe;

mod literal;

pub fn resolve_transaction(
    service: &RpcService,
    unresolved_transaction: &Transaction,
    reference_gas_price: u64,
    protocol_config: &ProtocolConfig,
) -> Result<TransactionData> {
    let sender = unresolved_transaction.sender().parse().map_err(|e| {
        FieldViolation::new("transaction.sender")
            .with_description(format!("invalid sender: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;

    let ptb = unresolved_transaction
        .kind
        .as_ref()
        .and_then(|kind| {
            kind.kind.as_ref().and_then(|kind| match kind {
                crate::proto::rpc::v2beta2::transaction_kind::Kind::ProgrammableTransaction(
                    ptb,
                ) => Some(ptb),
                _ => None,
            })
        })
        .ok_or_else(|| {
            FieldViolation::new("transaction.kind.programmable_transaction")
                .with_reason(ErrorReason::FieldMissing)
        })?;

    let commands = ptb
        .commands
        .iter()
        .map(sui_sdk_types::Command::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            FieldViolation::new("commands")
                .with_description(format!("invalid command: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let mut called_packages = called_packages(&service.reader, protocol_config, &commands)?;
    resolve_unresolved_transaction(
        &service.reader,
        &mut called_packages,
        reference_gas_price,
        protocol_config.max_tx_gas(),
        sender,
        &ptb.inputs,
        commands,
        unresolved_transaction.gas_payment.as_ref(),
    )
}

pub(super) struct NormalizedPackages {
    pool: normalized::RcPool,
    packages: HashMap<ObjectId, NormalizedPackage>,
}

struct NormalizedPackage {
    #[allow(unused)]
    package: MovePackage,
    normalized_modules: BTreeMap<String, normalized::Module<normalized::RcIdentifier>>,
}

pub(super) fn called_packages(
    reader: &StateReader,
    protocol_config: &ProtocolConfig,
    commands: &[Command],
) -> Result<NormalizedPackages> {
    let binary_config = sui_types::execution_config_utils::to_binary_config(protocol_config);
    let mut pool = normalized::RcPool::new();
    let mut packages = HashMap::new();

    for move_call in commands.iter().filter_map(|command| {
        if let Command::MoveCall(move_call) = command {
            Some(move_call)
        } else {
            None
        }
    }) {
        let package = reader
            .inner()
            .get_object(&(move_call.package.into()))
            .ok_or_else(|| ObjectNotFoundError::new(move_call.package))?
            .data
            .try_as_package()
            .ok_or_else(|| {
                RpcError::new(
                    tonic::Code::InvalidArgument,
                    format!("object {} is not a package", move_call.package),
                )
            })?
            .to_owned();

        // Normalization doesn't take the linkage or type origin tables into account, which means
        // that if you have an upgraded package that introduces a new type, then that type's
        // package ID is going to appear incorrectly if you fetch it from its normalized module.
        //
        // Despite the above this is safe given we are only using the signature information (and in
        // particular the reference kind) from the normalized package.
        let normalized_modules = package
            .normalize(&mut pool, &binary_config, /* include code */ true)
            .map_err(|e| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("unable to normalize package {}: {e}", move_call.package),
                )
            })?;
        let package = NormalizedPackage {
            package,
            normalized_modules,
        };

        packages.insert(move_call.package, package);
    }

    Ok(NormalizedPackages { pool, packages })
}

fn resolve_unresolved_transaction(
    reader: &StateReader,
    called_packages: &mut NormalizedPackages,
    reference_gas_price: u64,
    max_gas_budget: u64,
    sender: sui_sdk_types::Address,
    unresolved_inputs: &[crate::proto::rpc::v2beta2::Input],
    commands: Vec<Command>,
    gas_payment: Option<&crate::proto::rpc::v2beta2::GasPayment>,
) -> Result<TransactionData> {
    let gas_data = if let Some(unresolved_gas_payment) = gas_payment {
        let payment = unresolved_gas_payment
            .objects
            .iter()
            .map(|unresolved| resolve_object_reference(reader, unresolved.try_into()?))
            .collect::<Result<Vec<_>>>()?;
        GasData {
            payment,
            owner: unresolved_gas_payment.owner().parse().map_err(|e| {
                FieldViolation::new("owner")
                    .with_description(format!("invalid owner: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?,
            price: unresolved_gas_payment.price.unwrap_or(reference_gas_price),
            budget: unresolved_gas_payment.budget.unwrap_or(max_gas_budget),
        }
    } else {
        GasData {
            payment: vec![],
            owner: sender.into(),
            price: reference_gas_price,
            budget: max_gas_budget,
        }
    };

    //TODO handle expiration unresolved_transaction.expiration.into();
    let expiration = sui_types::transaction::TransactionExpiration::None;
    let ptb = resolve_ptb(reader, called_packages, unresolved_inputs, commands)?;
    Ok(TransactionData::V1(
        sui_types::transaction::TransactionDataV1 {
            kind: sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb),
            sender: sender.into(),
            gas_data,
            expiration,
        },
    ))
}

fn resolve_object_reference(
    reader: &StateReader,
    unresolved_object_reference: UnresolvedObjectReference,
) -> Result<ObjectRef> {
    let object = reader
        .inner()
        .get_object(&(unresolved_object_reference.object_id.into()))
        .ok_or_else(|| ObjectNotFoundError::new(unresolved_object_reference.object_id))?;
    resolve_object_reference_with_object(&object, unresolved_object_reference)
}

// Resolve an object reference against the provided object.
//
// Callers should check that the object_id matches the id in the `unresolved_object_reference`
// before calling.
fn resolve_object_reference_with_object(
    object: &sui_types::object::Object,
    unresolved_object_reference: UnresolvedObjectReference,
) -> Result<ObjectRef> {
    let UnresolvedObjectReference {
        object_id,
        version,
        digest,
    } = unresolved_object_reference;

    match object.owner() {
        sui_types::object::Owner::AddressOwner(_) | sui_types::object::Owner::Immutable => {}
        _ => {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                format!("object {object_id} is not Immutable or AddressOwned"),
            ))
        }
    }

    let id = object.id();
    let v = object.version();
    let d = object.digest();

    // This really should be an assert
    if object_id.inner() != &id.into_bytes() {
        return Err(RpcError::new(
            tonic::Code::Internal,
            "provided object and object_id should match",
        ));
    }

    if version.is_some_and(|version| version != v.value()) {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("provided version doesn't match, provided: {version:?} actual: {v}"),
        ));
    }

    if digest.is_some_and(|digest| digest.inner() != d.inner()) {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("provided digest doesn't match, provided: {digest:?} actual: {d}"),
        ));
    }

    Ok((id, v, d))
}

pub(super) fn resolve_ptb(
    reader: &StateReader,
    called_packages: &mut NormalizedPackages,
    unresolved_inputs: &[crate::proto::rpc::v2beta2::Input],
    commands: Vec<Command>,
) -> Result<ProgrammableTransaction> {
    let inputs = unresolved_inputs
        .iter()
        .enumerate()
        .map(|(arg_idx, arg)| resolve_arg(reader, called_packages, &commands, arg, arg_idx))
        .collect::<Result<_>>()?;

    ProgrammableTransaction {
        inputs,
        commands: commands.into_iter().map(Into::into).collect(),
    }
    .pipe(Ok)
}

fn resolve_arg(
    reader: &StateReader,
    called_packages: &mut NormalizedPackages,
    commands: &[Command],
    arg: &crate::proto::rpc::v2beta2::Input,
    arg_idx: usize,
) -> Result<CallArg> {
    use crate::proto::rpc::v2beta2::input::InputKind;

    match UnresolvedInput::from_proto(arg)? {
        // Pure, already prepared BCS input
        UnresolvedInput {
            kind: Some(InputKind::Pure),
            pure: Some(pure),
            object_id: None,
            version: None,
            digest: None,
            mutable: None,
            literal: None,
        }
        | UnresolvedInput {
            kind: None,
            pure: Some(pure),
            object_id: None,
            version: None,
            digest: None,
            mutable: None,
            literal: None,
        } => CallArg::Pure(pure.to_vec()),

        // Immutable or owned
        UnresolvedInput {
            kind: Some(InputKind::ImmutableOrOwned),
            pure: None,
            object_id: Some(object_id),
            version,
            digest,
            mutable: None,
            literal: None,
        } => CallArg::Object(ObjectArg::ImmOrOwnedObject(resolve_object_reference(
            reader,
            UnresolvedObjectReference {
                object_id,
                version,
                digest,
            },
        )?)),

        // Shared object
        UnresolvedInput {
            kind: Some(InputKind::Shared),
            pure: None,
            object_id: Some(object_id),
            version: _,
            digest: None,
            mutable: _,
            literal: None,
        } => CallArg::Object(resolve_shared_input(
            reader,
            called_packages,
            commands,
            arg_idx,
            object_id,
        )?),

        // Receiving
        UnresolvedInput {
            kind: Some(InputKind::Receiving),
            pure: None,
            object_id: Some(object_id),
            version,
            digest,
            mutable: None,
            literal: None,
        } => CallArg::Object(ObjectArg::Receiving(resolve_object_reference(
            reader,
            UnresolvedObjectReference {
                object_id,
                version,
                digest,
            },
        )?)),

        // Object, could be Immutable, Owned, Shared, or Receiving
        UnresolvedInput {
            kind: None,
            pure: None,
            object_id: Some(object_id),
            version,
            digest,
            mutable,
            literal: None,
        } => CallArg::Object(resolve_object(
            reader,
            called_packages,
            commands,
            arg_idx,
            object_id,
            version,
            digest,
            mutable,
        )?),

        // Literal, unresolved pure argument
        UnresolvedInput {
            kind: None, // TODO should we have a kind?
            pure: None,
            object_id: None,
            version: None,
            digest: None,
            mutable: None,
            literal: Some(literal),
        } => CallArg::Pure(literal::resolve_literal(
            called_packages,
            commands,
            arg_idx,
            literal,
        )?),

        _ => {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                "invalid unresolved input argument",
            ))
        }
    }
    .pipe(Ok)
}

fn resolve_object(
    reader: &StateReader,
    called_packages: &NormalizedPackages,
    commands: &[Command],
    arg_idx: usize,
    object_id: ObjectId,
    version: Option<sui_sdk_types::Version>,
    digest: Option<sui_sdk_types::ObjectDigest>,
    _mutable: Option<bool>,
) -> Result<ObjectArg> {
    let id = object_id.into();
    let object = reader
        .inner()
        .get_object(&id)
        .ok_or_else(|| ObjectNotFoundError::new(object_id))?;

    match object.owner() {
        sui_types::object::Owner::Immutable => resolve_object_reference_with_object(
            &object,
            UnresolvedObjectReference {
                object_id,
                version,
                digest,
            },
        )
        .map(ObjectArg::ImmOrOwnedObject),

        sui_types::object::Owner::AddressOwner(_) => {
            let object_ref = resolve_object_reference_with_object(
                &object,
                UnresolvedObjectReference {
                    object_id,
                    version,
                    digest,
                },
            )?;

            if is_input_argument_receiving(called_packages, commands, arg_idx)? {
                ObjectArg::Receiving(object_ref)
            } else {
                ObjectArg::ImmOrOwnedObject(object_ref)
            }
            .pipe(Ok)
        }
        sui_types::object::Owner::Shared { .. }
        | sui_types::object::Owner::ConsensusAddressOwner { .. } => {
            resolve_shared_input_with_object(called_packages, commands, arg_idx, object)
        }
        sui_types::object::Owner::ObjectOwner(_) => Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("object {object_id} is object owned and cannot be used as an input"),
        )),
    }
}

fn resolve_shared_input(
    reader: &StateReader,
    called_packages: &NormalizedPackages,
    commands: &[Command],
    arg_idx: usize,
    object_id: ObjectId,
) -> Result<ObjectArg> {
    let id = object_id.into();
    let object = reader
        .inner()
        .get_object(&id)
        .ok_or_else(|| ObjectNotFoundError::new(object_id))?;
    resolve_shared_input_with_object(called_packages, commands, arg_idx, object)
}

// Checks if the provided input argument is used as a receiving object
fn is_input_argument_receiving(
    called_packages: &NormalizedPackages,
    commands: &[Command],
    arg_idx: usize,
) -> Result<bool> {
    let (receiving_package, receiving_module, receiving_struct) =
        sui_types::transfer::RESOLVED_RECEIVING_STRUCT;

    let mut receiving = false;
    for (command, idx) in find_arg_uses(arg_idx, commands) {
        if let (Command::MoveCall(move_call), Some(idx)) = (command, idx) {
            let arg_type = arg_type_of_move_call_input(called_packages, move_call, idx)?;

            if let normalized::Type::Datatype(dt) = &*arg_type {
                if receiving_package == &dt.module.address
                    && receiving_module == dt.module.name.as_ref()
                    && receiving_struct == dt.name.as_ref()
                {
                    receiving = true;
                }
            }
        }

        //XXX do we want to ensure its only used once as receiving?
        if receiving {
            break;
        }
    }

    Ok(receiving)
}

// TODO still need to handle the case where a function parameter is a generic parameter and the
// real type needs to be lookedup from the provided type args in the MoveCall itself
fn arg_type_of_move_call_input(
    called_packages: &NormalizedPackages,
    move_call: &sui_sdk_types::MoveCall,
    idx: usize,
) -> Result<Rc<normalized::Type<normalized::RcIdentifier>>> {
    let function = called_packages
        .packages
        // Find the package
        .get(&move_call.package)
        // Find the module
        .and_then(|package| package.normalized_modules.get(move_call.module.as_str()))
        // Find the function
        .and_then(|module| module.functions.get(move_call.function.as_str()))
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::InvalidArgument,
                format!(
                    "unable to find function {package}::{module}::{function}",
                    package = move_call.package,
                    module = move_call.module,
                    function = move_call.function
                ),
            )
        })?;
    let Some(ty) = function.parameters.get(idx) else {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            "invalid input parameter",
        ));
    };
    Ok(ty.clone())
}

fn resolve_shared_input_with_object(
    called_packages: &NormalizedPackages,
    commands: &[Command],
    arg_idx: usize,
    object: sui_types::object::Object,
) -> Result<ObjectArg> {
    let object_id = object.id();
    let initial_shared_version = if let sui_types::object::Owner::Shared {
        initial_shared_version,
    }
    | sui_types::object::Owner::ConsensusAddressOwner {
        start_version: initial_shared_version,
        ..
    } = object.owner()
    {
        *initial_shared_version
    } else {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("object {object_id} is not a shared or consensus object"),
        ));
    };
    let mut mutable = false;
    for (command, idx) in find_arg_uses(arg_idx, commands) {
        match (command, idx) {
            (Command::MoveCall(move_call), Some(idx)) => {
                let arg_type = arg_type_of_move_call_input(called_packages, move_call, idx)?;
                if matches!(
                    &*arg_type,
                    normalized::Type::Reference(/* mut */ true, _) | normalized::Type::Datatype(_)
                ) {
                    mutable = true;
                }
            }
            (Command::SplitCoins(_) | Command::MergeCoins(_) | Command::MakeMoveVector(_), _) => {
                mutable = true;
            }
            _ => {}
        }
        // Early break out of the loop if we've already determined that the shared object
        // is needed to be mutable
        if mutable {
            break;
        }
    }

    Ok(ObjectArg::SharedObject {
        id: object_id,
        initial_shared_version,
        mutable,
    })
}

/// Given an particular input argument, find all of its uses.
///
/// The returned iterator contains all commands where the argument is used and an optional index
/// to indicate where the argument is used in that command.
fn find_arg_uses(
    arg_idx: usize,
    commands: &[Command],
) -> impl Iterator<Item = (&Command, Option<usize>)> {
    fn matches_input_arg(arg: Argument, arg_idx: usize) -> bool {
        matches!(arg, Argument::Input(idx) if idx as usize == arg_idx)
    }

    commands.iter().filter_map(move |command| {
        match command {
            Command::MoveCall(move_call) => move_call
                .arguments
                .iter()
                .position(|elem| matches_input_arg(*elem, arg_idx))
                .map(Some),
            Command::TransferObjects(transfer_objects) => {
                if matches_input_arg(transfer_objects.address, arg_idx) {
                    Some(None)
                } else {
                    transfer_objects
                        .objects
                        .iter()
                        .position(|elem| matches_input_arg(*elem, arg_idx))
                        .map(Some)
                }
            }
            Command::SplitCoins(split_coins) => {
                if matches_input_arg(split_coins.coin, arg_idx) {
                    Some(None)
                } else {
                    split_coins
                        .amounts
                        .iter()
                        .position(|amount| matches_input_arg(*amount, arg_idx))
                        .map(Some)
                }
            }
            Command::MergeCoins(merge_coins) => {
                if matches_input_arg(merge_coins.coin, arg_idx) {
                    Some(None)
                } else {
                    merge_coins
                        .coins_to_merge
                        .iter()
                        .position(|elem| matches_input_arg(*elem, arg_idx))
                        .map(Some)
                }
            }
            Command::Publish(_) => None,
            Command::MakeMoveVector(make_move_vector) => make_move_vector
                .elements
                .iter()
                .position(|elem| matches_input_arg(*elem, arg_idx))
                .map(Some),
            Command::Upgrade(upgrade) => matches_input_arg(upgrade.ticket, arg_idx).then_some(None),
        }
        .map(|x| (command, x))
    })
}

struct UnresolvedObjectReference {
    object_id: ObjectId,
    version: Option<sui_sdk_types::Version>,
    digest: Option<sui_sdk_types::ObjectDigest>,
}

impl TryFrom<&crate::proto::rpc::v2beta2::ObjectReference> for UnresolvedObjectReference {
    type Error = FieldViolation;

    fn try_from(value: &crate::proto::rpc::v2beta2::ObjectReference) -> Result<Self, Self::Error> {
        let object_id = value.object_id().parse().map_err(|e| {
            FieldViolation::new("object_id")
                .with_description(format!("invalid object_id: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        let version = value.version;
        let digest = value
            .digest
            .as_ref()
            .map(|digest| digest.parse())
            .transpose()
            .map_err(|e| {
                FieldViolation::new("digest")
                    .with_description(format!("invalid digest: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;

        Ok(Self {
            object_id,
            version,
            digest,
        })
    }
}

struct UnresolvedInput<'a> {
    pub kind: Option<crate::proto::rpc::v2beta2::input::InputKind>,
    pub pure: Option<&'a Bytes>,
    pub object_id: Option<sui_sdk_types::ObjectId>,
    pub version: Option<sui_sdk_types::Version>,
    pub digest: Option<sui_sdk_types::ObjectDigest>,
    pub mutable: Option<bool>,
    pub literal: Option<&'a prost_types::Value>,
}

impl<'a> UnresolvedInput<'a> {
    fn from_proto(input: &'a crate::proto::rpc::v2beta2::Input) -> Result<Self, FieldViolation> {
        Ok(Self {
            kind: input.kind.map(|_| input.kind()),
            literal: input.literal.as_deref(),
            pure: input.pure.as_ref(),
            object_id: input
                .object_id
                .as_ref()
                .map(|id| {
                    id.parse().map_err(|e| {
                        FieldViolation::new("object_id")
                            .with_description(format!("invalid object_id: {e}"))
                            .with_reason(ErrorReason::FieldInvalid)
                    })
                })
                .transpose()?,

            version: input.version,
            digest: input
                .digest
                .as_ref()
                .map(|digest| {
                    digest.parse().map_err(|e| {
                        FieldViolation::new("digest")
                            .with_description(format!("invalid digest: {e}"))
                            .with_reason(ErrorReason::FieldInvalid)
                    })
                })
                .transpose()?,
            mutable: input.mutable,
        })
    }
}
