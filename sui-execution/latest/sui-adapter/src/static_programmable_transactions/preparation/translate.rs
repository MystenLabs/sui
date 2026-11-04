// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::VerifiedPackageStore,
    execution_mode::ExecutionMode,
    execution_value::ExecutionState,
    static_programmable_transactions::{
        env::convert_vm_error,
        linkage::{
            config::ResolutionConfig,
            resolution::{
                LinkageMeter, ResolutionTable, VersionConstraint, add_and_unify, get_package,
            },
            resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
            single_linkage,
        },
        loading::ast::{DeserializedPackage, PackagePayload},
        metering::{self, translation_meter::TranslationMeter},
        preparation::ast::{self as A, Command},
    },
};
use move_binary_format::{
    CompiledModule,
    errors::{Location, VMResult},
};
use move_core_types::{language_storage::StructTag, resolver::IntraPackageName};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    Identifier, TypeTag,
    base_types::ObjectID,
    error::ExecutionErrorTrait,
    execution_status::{ExecutionErrorKind, TypeArgumentError},
    move_package::MovePackage,
    transaction::{self as P, CallArg, ObjectArg},
    type_input::{StructInput, TypeInput},
};

/// Preparation fails in input/command order, before VM-based type/signature validation. Upgrades
/// are analyzed after all ordinary contributions so their membership checks see the whole PTB.
pub fn transaction<Mode: ExecutionMode>(
    meter: &mut TranslationMeter<'_, '_>,
    config: &ResolutionConfig,
    store: &VerifiedPackageStore<'_>,
    state: &dyn ExecutionState,
    protocol_config: &ProtocolConfig,
    txn: P::ProgrammableTransaction,
) -> Result<A::Transaction, Mode::Error> {
    metering::pre_translation::meter::<Mode::Error>(meter, &txn)?;

    if let Err(err) = txn.validate_argument_indices() {
        invariant_violation!("PTB argument indices are checked at signing: {err}");
    }

    let P::ProgrammableTransaction { inputs, commands } = txn;
    let mut charge = || meter.charge_num_linkage_entries::<Mode::Error>(1);
    let mut table = config.resolution_table_with_native_packages(store, &mut charge)?;

    // First prepare inputs and fold their type constraints, even for inputs no command uses.
    let prepared_inputs = inputs
        .into_iter()
        .map(|arg| call_arg(arg, &mut table, store, state, &mut charge))
        .collect::<Result<Vec<_>, Mode::Error>>()?;

    // Second prepare commands and fold ordinary contributions in command order. Upgrade-init
    // analysis must wait until all input and ordinary command constraints are present.
    let prepared_commands = commands
        .into_iter()
        .enumerate()
        .map(|(i, cmd)| {
            let cmd = command::<Mode::Error>(cmd, config, store, protocol_config, &mut charge)
                .map_err(|e| e.with_command_index(i))?;
            single_linkage::analyze_command(&cmd, &mut table, store, &mut charge)
                .map_err(|e| e.with_command_index(i))?;
            Ok(cmd)
        })
        .collect::<Result<Vec<_>, Mode::Error>>()?;

    // Third apply upgrade-init constraints in command order. Membership and dependency equality
    // checks must see ordinary contributions even from commands that follow the upgrade.
    for (i, cmd) in prepared_commands.iter().enumerate() {
        if let Command::Upgrade(payload, _, current, _, linkage) = cmd {
            single_linkage::analyze_upgrade_command(
                payload,
                current,
                linkage,
                &mut table,
                store,
                protocol_config,
                &mut charge,
            )
            .map_err(|e| e.with_command_index(i))?;
        }
    }

    // Fourth verify every initializer's exact pins against the completed table. This must
    // precede finalization, which erases the distinction between Exact and AtLeast constraints.
    for (i, cmd) in prepared_commands.iter().enumerate() {
        single_linkage::validate_init_linkage_pinning::<Mode::Error>(cmd, &table, store)
            .map_err(|e| e.with_command_index(i))?;
    }

    let unified_linkage = ExecutableLinkage::new(ResolvedLinkage::from_resolution_table(table));
    unified_linkage.validate_runtime_linkage_consistency::<Mode::Error>()?;

    Ok(A::Transaction {
        inputs: prepared_inputs,
        commands: prepared_commands,
        unified_linkage,
    })
}

fn call_arg<E: ExecutionErrorTrait>(
    arg: CallArg,
    table: &mut ResolutionTable,
    store: &VerifiedPackageStore<'_>,
    state: &dyn ExecutionState,
    meter: &mut LinkageMeter<'_, E>,
) -> Result<CallArg, E> {
    match &arg {
        CallArg::Pure(_) | CallArg::Object(ObjectArg::Receiving(_)) => (),
        CallArg::Object(object_arg) => {
            let id = object_arg.id();
            let Some(obj) = state.read_object(&id) else {
                invariant_violation!("Object {id:?} not found in object store");
            };
            let Some(ty) = obj.type_() else {
                invariant_violation!("Object {id:?} does not have a Move type");
            };
            let tag: StructTag = ty.clone().into();
            single_linkage::add_type_packages(
                table,
                std::iter::once(&TypeTag::Struct(Box::new(tag))),
                store,
                meter,
            )?;
        }
        CallArg::FundsWithdrawal(arg) => {
            let P::WithdrawalTypeArg::Balance(tag) = &arg.type_arg;
            single_linkage::add_type_packages(table, std::iter::once(tag), store, meter)?;
        }
    }
    Ok(arg)
}

fn command<E: ExecutionErrorTrait>(
    cmd: P::Command,
    config: &ResolutionConfig,
    store: &VerifiedPackageStore<'_>,
    protocol_config: &ProtocolConfig,
    meter: &mut LinkageMeter<'_, E>,
) -> Result<Command, E> {
    Ok(match cmd {
        P::Command::MoveCall(call) => {
            let P::ProgrammableMoveCall {
                package,
                module,
                function,
                type_arguments,
                arguments,
            } = *call;
            let type_arguments = type_arguments
                .into_iter()
                .enumerate()
                .map(|(i, ty)| normalize_type(i, ty, store))
                .collect::<Result<Vec<_>, E>>()?;
            let module = to_identifier(module)?;
            let function = to_identifier(function)?;
            let pkg = get_package(&package, store)?;
            let not_found = || {
                E::new_with_source(
                    ExecutionErrorKind::FunctionNotFound,
                    format!(
                        "Could not resolve function '{function}' in module '{package}::{module}'"
                    ),
                )
            };
            let visibility = pkg
                .modules()
                .iter()
                .find(|(id, _)| id.name() == module.as_ident_str())
                .ok_or_else(not_found)?
                .1
                .compiled_module()
                .find_function_def_by_name(function.as_str())
                .ok_or_else(not_found)?
                .1
                .visibility;
            Command::MoveCall(Box::new(A::MoveCall {
                package,
                module,
                function,
                visibility,
                type_arguments,
                arguments,
            }))
        }
        P::Command::MakeMoveVec(ty, args) => {
            Command::MakeMoveVec(ty.map(|ty| normalize_type(0, ty, store)).transpose()?, args)
        }
        P::Command::TransferObjects(args, recipient) => Command::TransferObjects(args, recipient),
        P::Command::SplitCoins(coin, amounts) => Command::SplitCoins(coin, amounts),
        P::Command::MergeCoins(coin, coins) => Command::MergeCoins(coin, coins),
        P::Command::Publish(bytes, deps) => {
            let linkage = publication_linkage(config, &deps, store, meter)?;
            let payload = deserialize_package(&bytes, &deps, store, protocol_config)?;
            Command::Publish(payload, deps, linkage)
        }
        P::Command::Upgrade(bytes, deps, current, ticket) => {
            let linkage = publication_linkage(config, &deps, store, meter)?;
            let payload = deserialize_package(&bytes, &deps, store, protocol_config)?;
            Command::Upgrade(payload, deps, current, ticket, linkage)
        }
    })
}

fn publication_linkage<E: ExecutionErrorTrait>(
    config: &ResolutionConfig,
    deps: &[ObjectID],
    store: &VerifiedPackageStore<'_>,
    meter: &mut LinkageMeter<'_, E>,
) -> Result<ResolvedLinkage, E> {
    let mut table = config.resolution_table_with_native_packages(store, meter)?;
    for id in deps {
        add_and_unify(id, store, &mut table, VersionConstraint::exact, meter)?;
    }
    Ok(ResolvedLinkage::from_resolution_table(table))
}

// Recursively normalize a type input into a type tag (defining ID based).
//
// User MoveCall and MakeMoveVec types pass the iterative `type_input_validity_check` during
// transaction validity checking (including dev-inspect), bounding recursion depth and total
// type nodes here. System/genesis PTBs are trusted internal constructions.
fn normalize_type<E: ExecutionErrorTrait>(
    type_arg_idx: usize,
    ty: TypeInput,
    store: &VerifiedPackageStore<'_>,
) -> Result<TypeTag, E> {
    Ok(match ty {
        TypeInput::Bool => TypeTag::Bool,
        TypeInput::U8 => TypeTag::U8,
        TypeInput::U16 => TypeTag::U16,
        TypeInput::U32 => TypeTag::U32,
        TypeInput::U64 => TypeTag::U64,
        TypeInput::U128 => TypeTag::U128,
        TypeInput::U256 => TypeTag::U256,
        TypeInput::Address => TypeTag::Address,
        TypeInput::Signer => TypeTag::Signer,
        TypeInput::Vector(type_input) => {
            let inner = normalize_type(type_arg_idx, *type_input, store)?;
            TypeTag::Vector(Box::new(inner))
        }
        TypeInput::Struct(struct_input) => {
            let StructInput {
                address,
                module,
                name,
                type_params,
            } = *struct_input;

            let pkg = store
                .get_package(&address.into())
                .ok()
                .flatten()
                .ok_or_else(|| {
                    let argument_idx = match checked_as!(type_arg_idx, u16) {
                        Err(e) => return e.into(),
                        Ok(v) => v,
                    };
                    E::from_kind(ExecutionErrorKind::TypeArgumentError {
                        argument_idx,
                        kind: TypeArgumentError::TypeNotFound,
                    })
                })?;
            let module = to_identifier(module)?;
            let name = to_identifier(name)?;
            let tid = IntraPackageName {
                module_name: module,
                type_name: name,
            };
            let Some(resolved_address) = pkg.type_origin_table().get(&tid).cloned() else {
                return Err(E::from_kind(ExecutionErrorKind::TypeArgumentError {
                    argument_idx: checked_as!(type_arg_idx, u16)?,
                    kind: TypeArgumentError::TypeNotFound,
                }));
            };

            let tys = type_params
                .into_iter()
                .map(|tp| normalize_type(type_arg_idx, tp, store))
                .collect::<Result<Vec<_>, _>>()?;
            TypeTag::Struct(Box::new(StructTag {
                address: resolved_address,
                module: tid.module_name,
                name: tid.type_name,
                type_params: tys,
            }))
        }
    })
}

fn deserialize_package<E: ExecutionErrorTrait>(
    module_bytes: &[Vec<u8>],
    dep_ids: &[ObjectID],
    store: &VerifiedPackageStore<'_>,
    protocol_config: &ProtocolConfig,
) -> Result<PackagePayload, E> {
    assert_invariant!(
        !module_bytes.is_empty(),
        "empty package is checked in transaction input checker"
    );

    let total_bytes = module_bytes.iter().map(|v| v.len()).sum();

    let binary_config = protocol_config.binary_config(None);
    let deserialized_modules = module_bytes
        .iter()
        .map(|b| {
            CompiledModule::deserialize_with_config(b, &binary_config)
                .map_err(|e| e.finish(Location::Undefined))
        })
        .collect::<VMResult<Vec<CompiledModule>>>()
        .map_err(|e| convert_vm_error::<E>(e, store, None, protocol_config))?;
    let computed_digest = MovePackage::compute_digest_for_modules_and_deps(
        module_bytes,
        dep_ids,
        /* hash_modules */ true,
    );
    Ok(PackagePayload::Deserialized(DeserializedPackage::new(
        deserialized_modules,
        total_bytes,
        computed_digest,
    )))
}

fn to_identifier<E: ExecutionErrorTrait>(name: String) -> Result<Identifier, E> {
    Identifier::new(name)
        .map_err(|e| E::new_with_source(ExecutionErrorKind::VMInvariantViolation, e.to_string()))
}
