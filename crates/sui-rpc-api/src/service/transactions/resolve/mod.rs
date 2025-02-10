// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::reader::StateReader;
use crate::service::objects::ObjectNotFoundError;
use crate::types::ResolveTransactionQueryParameters;
use crate::types::ResolveTransactionResponse;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use itertools::Itertools;
use move_binary_format::normalized;
use sui_protocol_config::ProtocolConfig;
use sui_sdk_transaction_builder::unresolved;
use sui_sdk_types::Argument;
use sui_sdk_types::Command;
use sui_sdk_types::ObjectId;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;
use sui_types::move_package::MovePackage;
use sui_types::transaction::CallArg;
use sui_types::transaction::GasData;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::ProgrammableTransaction;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionDataAPI;
use tap::Pipe;

mod literal;

impl RpcService {
    pub fn resolve_transaction(
        &self,
        parameters: ResolveTransactionQueryParameters,
        unresolved_transaction: unresolved::Transaction,
    ) -> Result<ResolveTransactionResponse> {
        let executor = self
            .executor
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No Transaction Executor"))?;
        let (reference_gas_price, protocol_config) = {
            let system_state = self.reader.get_system_state_summary()?;

            let current_protocol_version = system_state.protocol_version;

            let protocol_config = ProtocolConfig::get_for_version_if_supported(
                current_protocol_version.into(),
                self.reader.inner().get_chain_identifier()?.chain(),
            )
            .ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    "unable to get current protocol config",
                )
            })?;

            (system_state.reference_gas_price, protocol_config)
        };
        let called_packages =
            called_packages(&self.reader, &protocol_config, &unresolved_transaction)?;
        let user_provided_budget = unresolved_transaction
            .gas_payment
            .as_ref()
            .and_then(|payment| payment.budget);
        let mut resolved_transaction = resolve_unresolved_transaction(
            &self.reader,
            &called_packages,
            reference_gas_price,
            protocol_config.max_tx_gas(),
            unresolved_transaction,
        )?;

        // If the user didn't provide a budget we need to run a quick simulation in order to calculate
        // a good estimated budget to use
        let budget = if let Some(user_provided_budget) = user_provided_budget {
            user_provided_budget
        } else {
            let simulation_result = executor
                .simulate_transaction(resolved_transaction.clone())
                .map_err(anyhow::Error::from)?;

            let estimate = estimate_gas_budget_from_gas_cost(
                simulation_result.effects.gas_cost_summary(),
                reference_gas_price,
            );
            resolved_transaction.gas_data_mut().budget = estimate;
            estimate
        };

        // If the user didn't provide any gas payment we need to do gas selection now
        if resolved_transaction.gas_data().payment.is_empty() {
            let input_objects = resolved_transaction
                .input_objects()
                .map_err(anyhow::Error::from)?
                .iter()
                .flat_map(|obj| match obj {
                    sui_types::transaction::InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => {
                        Some(*id)
                    }
                    _ => None,
                })
                .collect_vec();
            let gas_coins = select_gas(
                &self.reader,
                resolved_transaction.gas_data().owner,
                budget,
                protocol_config.max_gas_payment_objects(),
                &input_objects,
            )?;
            resolved_transaction.gas_data_mut().payment = gas_coins;
        }

        let simulation = if parameters.simulate {
            self.simulate_transaction(
                &parameters.simulate_transaction_parameters,
                resolved_transaction.clone().try_into()?,
            )?
            .pipe(Some)
        } else {
            None
        };

        ResolveTransactionResponse {
            transaction: resolved_transaction.try_into()?,
            simulation,
        }
        .pipe(Ok)
    }
}

struct NormalizedPackage {
    #[allow(unused)]
    package: MovePackage,
    normalized_modules: BTreeMap<String, normalized::Module>,
}

fn called_packages(
    reader: &StateReader,
    protocol_config: &ProtocolConfig,
    unresolved_transaction: &unresolved::Transaction,
) -> Result<HashMap<ObjectId, NormalizedPackage>> {
    let binary_config = sui_types::execution_config_utils::to_binary_config(protocol_config);
    let mut packages = HashMap::new();

    for move_call in unresolved_transaction
        .ptb
        .commands
        .iter()
        .filter_map(|command| {
            if let Command::MoveCall(move_call) = command {
                Some(move_call)
            } else {
                None
            }
        })
    {
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
        let normalized_modules = package.normalize(&binary_config).map_err(|e| {
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

    Ok(packages)
}

fn resolve_unresolved_transaction(
    reader: &StateReader,
    called_packages: &HashMap<ObjectId, NormalizedPackage>,
    reference_gas_price: u64,
    max_gas_budget: u64,
    unresolved_transaction: unresolved::Transaction,
) -> Result<TransactionData> {
    let sender = unresolved_transaction.sender.into();
    let gas_data = if let Some(unresolved_gas_payment) = unresolved_transaction.gas_payment {
        let payment = unresolved_gas_payment
            .objects
            .into_iter()
            .map(|unresolved| resolve_object_reference(reader, unresolved))
            .collect::<Result<Vec<_>>>()?;
        GasData {
            payment,
            owner: unresolved_gas_payment.owner.into(),
            price: unresolved_gas_payment.price.unwrap_or(reference_gas_price),
            budget: unresolved_gas_payment.budget.unwrap_or(max_gas_budget),
        }
    } else {
        GasData {
            payment: vec![],
            owner: sender,
            price: reference_gas_price,
            budget: max_gas_budget,
        }
    };
    let expiration = unresolved_transaction.expiration.into();
    let ptb = resolve_ptb(reader, called_packages, unresolved_transaction.ptb)?;
    Ok(TransactionData::V1(
        sui_types::transaction::TransactionDataV1 {
            kind: sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb),
            sender,
            gas_data,
            expiration,
        },
    ))
}

fn resolve_object_reference(
    reader: &StateReader,
    unresolved_object_reference: unresolved::ObjectReference,
) -> Result<ObjectRef> {
    let object_id = unresolved_object_reference.object_id;
    let object = reader
        .inner()
        .get_object(&object_id.into())
        .ok_or_else(|| ObjectNotFoundError::new(object_id))?;
    resolve_object_reference_with_object(&object, unresolved_object_reference)
}

// Resolve an object reference against the provided object.
//
// Callers should check that the object_id matches the id in the `unresolved_object_reference`
// before calling.
fn resolve_object_reference_with_object(
    object: &sui_types::object::Object,
    unresolved_object_reference: unresolved::ObjectReference,
) -> Result<ObjectRef> {
    let unresolved::ObjectReference {
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

fn resolve_ptb(
    reader: &StateReader,
    called_packages: &HashMap<ObjectId, NormalizedPackage>,
    unresolved_ptb: unresolved::ProgrammableTransaction,
) -> Result<ProgrammableTransaction> {
    let inputs = unresolved_ptb
        .inputs
        .into_iter()
        .enumerate()
        .map(|(arg_idx, arg)| {
            resolve_arg(
                reader,
                called_packages,
                &unresolved_ptb.commands,
                arg,
                arg_idx,
            )
        })
        .collect::<Result<_>>()?;

    ProgrammableTransaction {
        inputs,
        commands: unresolved_ptb
            .commands
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?,
    }
    .pipe(Ok)
}

fn resolve_arg(
    reader: &StateReader,
    called_packages: &HashMap<ObjectId, NormalizedPackage>,
    commands: &[Command],
    arg: unresolved::Input,
    arg_idx: usize,
) -> Result<CallArg> {
    use fastcrypto::encoding::Base64;
    use fastcrypto::encoding::Encoding;
    use sui_sdk_transaction_builder::unresolved::InputKind::*;

    let unresolved::Input {
        kind,
        value,
        object_id,
        version,
        digest,
        mutable,
    } = arg;

    match (kind, value, object_id, version, digest, mutable) {
        // pre serialized BCS input encoded as a base64 string
        (Some(Pure), Some(unresolved::Value::String(v)), None, None, None, None) => {
            let value = Base64::decode(&v).map_err(|e| {
                RpcError::new(
                    tonic::Code::InvalidArgument,
                    format!("argument is an invalid pure argument: {e}"),
                )
            })?;
            CallArg::Pure(value)
        }
        // pre serialized BCS input encoded as a a JSON array of u8s
        (Some(Pure), Some(array @ unresolved::Value::Array(_)), None, None, None, None) => {
            let value = serde_json::from_value(serde_json::Value::from(array)).map_err(|e| {
                RpcError::new(
                    tonic::Code::InvalidArgument,
                    format!("argument is an invalid pure argument: {e}"),
                )
            })?;
            CallArg::Pure(value)
        }

        // Literal, unresolved pure argument
        (Some(Literal), Some(value), None, None, None, None)
        | (None, Some(value), None, None, None, None) => CallArg::Pure(literal::resolve_literal(
            called_packages,
            commands,
            arg_idx,
            value,
        )?),

        // Immutable or owned
        (Some(ImmutableOrOwned), None, Some(object_id), version, digest, None) => {
            CallArg::Object(ObjectArg::ImmOrOwnedObject(resolve_object_reference(
                reader,
                unresolved::ObjectReference {
                    object_id,
                    version,
                    digest,
                },
            )?))
        }

        // Shared object
        (Some(Shared), None, Some(object_id), _version, None, _mutable) => CallArg::Object(
            resolve_shared_input(reader, called_packages, commands, arg_idx, object_id)?,
        ),

        // Receiving
        (Some(Receiving), None, Some(object_id), version, digest, None) => {
            CallArg::Object(ObjectArg::Receiving(resolve_object_reference(
                reader,
                unresolved::ObjectReference {
                    object_id,
                    version,
                    digest,
                },
            )?))
        }

        // Object, could be Immutable, Owned, Shared, or Receiving
        (None, None, Some(object_id), version, digest, mutable) => CallArg::Object(resolve_object(
            reader,
            called_packages,
            commands,
            arg_idx,
            object_id,
            version,
            digest,
            mutable,
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
    called_packages: &HashMap<ObjectId, NormalizedPackage>,
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
            unresolved::ObjectReference {
                object_id,
                version,
                digest,
            },
        )
        .map(ObjectArg::ImmOrOwnedObject),

        sui_types::object::Owner::AddressOwner(_) => {
            let object_ref = resolve_object_reference_with_object(
                &object,
                unresolved::ObjectReference {
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
        sui_types::object::Owner::Shared { .. } | sui_types::object::Owner::ConsensusV2 { .. } => {
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
    called_packages: &HashMap<ObjectId, NormalizedPackage>,
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
    called_packages: &HashMap<ObjectId, NormalizedPackage>,
    commands: &[Command],
    arg_idx: usize,
) -> Result<bool> {
    let (receiving_package, receiving_module, receiving_struct) =
        sui_types::transfer::RESOLVED_RECEIVING_STRUCT;

    let mut receiving = false;
    for (command, idx) in find_arg_uses(arg_idx, commands) {
        if let (Command::MoveCall(move_call), Some(idx)) = (command, idx) {
            let arg_type = arg_type_of_move_call_input(called_packages, move_call, idx)?;

            if let move_binary_format::normalized::Type::Struct {
                address,
                module,
                name,
                ..
            } = arg_type
            {
                if receiving_package == address
                    && receiving_module == module.as_ref()
                    && receiving_struct == name.as_ref()
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
fn arg_type_of_move_call_input<'a>(
    called_packages: &'a HashMap<ObjectId, NormalizedPackage>,
    move_call: &sui_sdk_types::MoveCall,
    idx: usize,
) -> Result<&'a move_binary_format::normalized::Type> {
    let function = called_packages
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
    function
        .parameters
        .get(idx)
        .ok_or_else(|| RpcError::new(tonic::Code::InvalidArgument, "invalid input parameter"))
}

fn resolve_shared_input_with_object(
    called_packages: &HashMap<ObjectId, NormalizedPackage>,
    commands: &[Command],
    arg_idx: usize,
    object: sui_types::object::Object,
) -> Result<ObjectArg> {
    let object_id = object.id();
    let initial_shared_version = if let sui_types::object::Owner::Shared {
        initial_shared_version,
    }
    | sui_types::object::Owner::ConsensusV2 {
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
                    arg_type,
                    move_binary_format::normalized::Type::MutableReference(_)
                        | move_binary_format::normalized::Type::Struct { .. }
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

/// Estimate the gas budget using the gas_cost_summary from a previous DryRun
///
/// The estimated gas budget is computed as following:
/// * the maximum between A and B, where:
///     A = computation cost + GAS_SAFE_OVERHEAD * reference gas price
///     B = computation cost + storage cost - storage rebate + GAS_SAFE_OVERHEAD * reference gas price
///     overhead
///
/// This gas estimate is computed similarly as in the TypeScript SDK
fn estimate_gas_budget_from_gas_cost(
    gas_cost_summary: &GasCostSummary,
    reference_gas_price: u64,
) -> u64 {
    const GAS_SAFE_OVERHEAD: u64 = 1000;

    let safe_overhead = GAS_SAFE_OVERHEAD * reference_gas_price;
    let computation_cost_with_overhead = gas_cost_summary.computation_cost + safe_overhead;

    let gas_usage = gas_cost_summary.net_gas_usage() + safe_overhead as i64;
    computation_cost_with_overhead.max(if gas_usage < 0 { 0 } else { gas_usage as u64 })
}

fn select_gas(
    reader: &StateReader,
    owner: SuiAddress,
    budget: u64,
    max_gas_payment_objects: u32,
    input_objects: &[ObjectID],
) -> Result<Vec<ObjectRef>> {
    //TODO implement index of gas coins sorted in order of decreasing value
    let gas_coins = reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?
        .account_owned_objects_info_iter(owner, None)?
        .filter(|info| info.type_.is_gas_coin())
        .filter(|info| !input_objects.contains(&info.object_id))
        .filter_map(|info| reader.inner().get_object(&info.object_id))
        .filter_map(|object| {
            GasCoin::try_from(&object)
                .ok()
                .map(|coin| (object.compute_object_reference(), coin.value()))
        })
        .take(max_gas_payment_objects as usize);

    let mut selected_gas = vec![];
    let mut selected_gas_value = 0;

    for (object_ref, value) in gas_coins {
        selected_gas.push(object_ref);
        selected_gas_value += value;
    }

    if selected_gas_value >= budget {
        Ok(selected_gas)
    } else {
        Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!(
                "unable to select sufficient gas coins from account {owner} \
                    to satisfy required budget {budget}"
            ),
        ))
    }
}
