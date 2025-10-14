// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Execution module for replay.
//! The call to the executor `execute_transaction_to_effects` is here
//! and the logic to call it is pretty straightforward.
//! `execute_transaction_to_effects` requires info from the `EpochStore`
//! (epoch, protocol config, epoch start timestamp, rgp), from the `TransactionStore`
//! as in transaction data and effects, and from the `ObjectStore` for dynamic loads
//! (e.g. dynamic fields).
//! This module also contains the traits used by execution to talk to
//! the store (BackingPackageStore, ObjectStore, ChildObjectResolver)

use crate::{
    replay_interface::{EpochStore, ObjectKey, ObjectStore, VersionQuery},
    replay_txn::{get_input_objects_for_replay, ReplayTransaction},
};
use anyhow::Context;
use move_binary_format::{
    binary_config::BinaryConfig,
    file_format::{CompiledModule, SignatureToken},
};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag},
    resolver::ModuleResolver,
};
use move_trace_format::format::MoveTraceBuilder;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
    sync::Arc,
};
use sui_execution::Executor;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, VersionNumber},
    committee::EpochId,
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::{ExecutionError, SuiError, SuiResult},
    execution_params::{get_early_execution_error, BalanceWithdrawStatus, ExecutionOrEarlyError},
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    metrics::LimitsMetrics,
    object::Object,
    storage::{BackingPackageStore, ChildObjectResolver, PackageObject, ParentSync},
    supported_protocol_versions::ProtocolConfig,
    transaction::{
        CheckedInputObjects, ProgrammableTransaction, TransactionData, TransactionDataAPI,
    },
    type_input::TypeInput,
};
use tracing::{debug, debug_span, trace};

// Executor for the replay. Created and used by `ReplayTransaction`.
pub struct ReplayExecutor {
    protocol_config: ProtocolConfig,
    executor: Arc<dyn Executor + Send + Sync>,
    metrics: Arc<LimitsMetrics>,
}

// Returned struct from execution. Contains all the data related to a transaction.
// Transaction data and effects (both expected and actual) and the caches containing
// the objects used during execution.
pub struct TxnContextAndEffects {
    pub txn_data: TransactionData,             // original transaction data
    pub execution_effects: TransactionEffects, // effects of the replay execution
    pub expected_effects: TransactionEffects,  // expected effects as found in the transaction data
    pub gas_status: SuiGasStatus,              // gas status of the replay execution
    pub object_cache: BTreeMap<ObjectID, BTreeMap<u64, Object>>, // object cache
    pub inner_store: InnerTemporaryStore,      // temporary store used during execution
    pub checkpoint: u64,                       // checkpoint where the transaction was included
    pub protocol_version: u64,                 // protocol version used for execution
}

/// Detailed information about a Move package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub published_id: ObjectID,
    pub original_id: ObjectID,
    pub module_names: Vec<String>,
}

/// Type of object in the replay cache with detailed Move information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectType {
    /// A Move package containing compiled modules
    Package(PackageInfo),
    /// A Move object with its struct tag
    MoveObject(StructTag),
}

/// Entry in the replay cache summary representing an object accessed during replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub object_id: ObjectID,
    pub version: u64,
    pub object_type: ObjectType,
}

/// Compact representation of the replay cache for serialization.
/// Contains the execution context (epoch_id from transaction effects, checkpoint from transaction info)
/// and a list of all objects accessed during replay, with their type information but without object content.
/// The epoch_id represents the epoch in which the original transaction was executed.
/// The checkpoint represents the checkpoint sequence number where the transaction was included.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayCacheSummary {
    pub epoch_id: u64,
    pub checkpoint: u64,
    pub network: String,
    pub protocol_version: u64,
    /// List of objects accessed during replay with their version and type information
    pub cache_entries: Vec<CacheEntry>,
}

impl ReplayCacheSummary {
    /// Create a ReplayCacheSummary from the object cache, extracting detailed Move information.
    pub fn from_cache(
        epoch_id: u64,
        checkpoint: u64,
        network: String,
        protocol_version: u64,
        object_cache: &BTreeMap<ObjectID, BTreeMap<u64, Object>>,
    ) -> Self {
        let mut cache_entries = Vec::new();

        for (object_id, versions) in object_cache {
            for (version, object) in versions {
                let object_type = if object.is_package() {
                    // Extract package information
                    let package = object
                        .data
                        .try_as_package()
                        .expect("Package object should have package data");
                    let package_info = PackageInfo {
                        published_id: package.id(),
                        original_id: package.original_package_id(),
                        module_names: package.serialized_module_map().keys().cloned().collect(),
                    };
                    ObjectType::Package(package_info)
                } else {
                    // Extract Move object struct tag
                    let struct_tag = object
                        .struct_tag()
                        .expect("Move object should have struct tag");
                    ObjectType::MoveObject(struct_tag)
                };

                cache_entries.push(CacheEntry {
                    object_id: *object_id,
                    version: *version,
                    object_type,
                });
            }
        }

        Self {
            epoch_id,
            checkpoint,
            network,
            protocol_version,
            cache_entries,
        }
    }
}

/// Datatype definition: (address, module, name, variants)
pub type Datatype = (AccountAddress, String, String, Vec<MoveType>);

/// Custom Move type representation for JSON serialization.
/// This provides the exact type information we want to expose in the JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Vector(Box<MoveType>),
    Datatype(Datatype),
    DatatypeInstantiation(Box<(Datatype, Vec<MoveType>)>),
    Reference(Box<MoveType>),
    MutableReference(Box<MoveType>),
    TypeParameter(u16),
}

/// Function signature information for MoveCall commands in a ProgrammableTransaction.
/// Contains detailed parameter and return type information for each function call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSignature {
    /// Package ID where the function is defined
    pub package: ObjectID,
    /// Module name containing the function
    pub module: String,
    /// Function name
    pub function: String,
    /// Parameter types
    pub parameters: Vec<MoveType>,
    /// Return types
    pub return_types: Vec<MoveType>,
}

/// Move call information containing extracted function signatures.
/// This provides type information for all commands in the transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveCallInfo {
    /// Vector of function signatures, one for each command
    /// None for non-MoveCall commands, Some(signature) for MoveCall commands
    pub command_signatures: Vec<Option<FunctionSignature>>,
}

impl MoveCallInfo {
    /// Create MoveCallInfo by extracting function signatures from a ProgrammableTransaction.
    /// Creates a vector with one entry per command, None for non-MoveCall commands.
    pub fn from_transaction(
        ptb: &ProgrammableTransaction,
        object_cache: &BTreeMap<ObjectID, BTreeMap<u64, Object>>,
    ) -> anyhow::Result<Self> {
        let mut command_signatures = Vec::with_capacity(ptb.commands.len());

        for command in ptb.commands.iter() {
            let signature = if let sui_types::transaction::Command::MoveCall(move_call) = command {
                // Extract function signature from the MoveCall
                match Self::extract_function_signature(move_call, object_cache) {
                    Ok(signature) => {
                        tracing::debug!(
                            "Successfully extracted signature for {}::{}::{}",
                            signature.package,
                            signature.module,
                            signature.function
                        );
                        Some(signature)
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to extract signature for {}::{}::{}: {}",
                            move_call.package,
                            move_call.module,
                            move_call.function,
                            e
                        );
                        None
                    }
                }
            } else {
                None
            };
            command_signatures.push(signature);
        }

        Ok(MoveCallInfo { command_signatures })
    }

    /// Extract function signature information from a MoveCall command.
    fn extract_function_signature(
        move_call: &sui_types::transaction::ProgrammableMoveCall,
        object_cache: &BTreeMap<ObjectID, BTreeMap<u64, Object>>,
    ) -> anyhow::Result<FunctionSignature> {
        let package_id = move_call.package;
        let module_name = move_call.module.as_str();
        let function_name = move_call.function.as_str();

        // Find the package in the object cache
        let package_obj = object_cache
            .get(&package_id)
            .and_then(|versions| versions.values().next())
            .ok_or_else(|| anyhow::anyhow!("Package {} not found in cache", package_id))?;

        // Extract MovePackage from the object
        let move_package = package_obj
            .data
            .try_as_package()
            .ok_or_else(|| anyhow::anyhow!("Object {} is not a package", package_id))?;

        // Get the module bytecode from the package
        let module_bytes = move_package
            .serialized_module_map()
            .get(module_name)
            .ok_or_else(|| {
                anyhow::anyhow!("Module {} not found in package {}", module_name, package_id)
            })?;

        // Deserialize the module
        let binary_config = BinaryConfig::standard();
        let compiled_module = CompiledModule::deserialize_with_config(module_bytes, &binary_config)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize module {}: {}", module_name, e))?;

        // Find the function definition
        let (_, function_def) = compiled_module
            .find_function_def_by_name(function_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Function {} not found in module {}::{}",
                    function_name,
                    package_id,
                    module_name
                )
            })?;

        // Get the function handle
        let function_handle = compiled_module.function_handle_at(function_def.function);

        // Get parameter and return signatures
        let param_signature = compiled_module.signature_at(function_handle.parameters);
        let return_signature = compiled_module.signature_at(function_handle.return_);

        // Convert TypeInputs to MoveTypes for signature processing
        let type_arguments_as_move_types: Vec<MoveType> = move_call
            .type_arguments
            .iter()
            .map(Self::type_input_to_move_type)
            .collect();

        // Convert SignatureTokens to MoveTypes
        let parameters = param_signature
            .0
            .iter()
            .map(|token| {
                Self::signature_token_to_move_type(
                    token,
                    &compiled_module,
                    &type_arguments_as_move_types,
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let return_types = return_signature
            .0
            .iter()
            .map(|token| {
                Self::signature_token_to_move_type(
                    token,
                    &compiled_module,
                    &type_arguments_as_move_types,
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(FunctionSignature {
            package: package_id,
            module: module_name.to_string(),
            function: function_name.to_string(),
            parameters,
            return_types,
        })
    }

    /// Convert TypeInput to MoveType.
    /// TypeInput comes from the transaction's type arguments.
    fn type_input_to_move_type(type_input: &TypeInput) -> MoveType {
        match type_input {
            TypeInput::Bool => MoveType::Bool,
            TypeInput::U8 => MoveType::U8,
            TypeInput::U16 => MoveType::U16,
            TypeInput::U32 => MoveType::U32,
            TypeInput::U64 => MoveType::U64,
            TypeInput::U128 => MoveType::U128,
            TypeInput::U256 => MoveType::U256,
            TypeInput::Address => MoveType::Address,
            TypeInput::Signer => MoveType::Address, // Signer is treated as Address
            TypeInput::Vector(element) => {
                MoveType::Vector(Box::new(Self::type_input_to_move_type(element)))
            }
            TypeInput::Struct(struct_input) => {
                let type_params: Vec<MoveType> = struct_input
                    .type_params
                    .iter()
                    .map(Self::type_input_to_move_type)
                    .collect();

                let datatype = (
                    struct_input.address,
                    struct_input.module.clone(),
                    struct_input.name.clone(),
                    vec![], // Empty variants - we ignore enum variants
                );

                if type_params.is_empty() {
                    // Non-generic datatype
                    MoveType::Datatype(datatype)
                } else {
                    // Generic instantiation
                    MoveType::DatatypeInstantiation(Box::new((datatype, type_params)))
                }
            }
        }
    }

    /// Convert SignatureToken to MoveType.
    /// This is the core conversion that bridges Move's type system with our custom representation.
    fn signature_token_to_move_type(
        token: &SignatureToken,
        module: &CompiledModule,
        type_arguments: &[MoveType],
    ) -> anyhow::Result<MoveType> {
        match token {
            SignatureToken::Bool => Ok(MoveType::Bool),
            SignatureToken::U8 => Ok(MoveType::U8),
            SignatureToken::U16 => Ok(MoveType::U16),
            SignatureToken::U32 => Ok(MoveType::U32),
            SignatureToken::U64 => Ok(MoveType::U64),
            SignatureToken::U128 => Ok(MoveType::U128),
            SignatureToken::U256 => Ok(MoveType::U256),
            SignatureToken::Address => Ok(MoveType::Address),
            SignatureToken::Signer => Ok(MoveType::Address), // Signer is treated as Address
            SignatureToken::Vector(element_type) => {
                let element =
                    Self::signature_token_to_move_type(element_type, module, type_arguments)?;
                Ok(MoveType::Vector(Box::new(element)))
            }
            SignatureToken::Datatype(datatype_handle_idx) => {
                let datatype_handle = module.datatype_handle_at(*datatype_handle_idx);
                let module_handle = module.module_handle_at(datatype_handle.module);
                let address = *module.address_identifier_at(module_handle.address);
                let module_name = module.identifier_at(module_handle.name).to_string();
                let struct_name = module.identifier_at(datatype_handle.name).to_string();

                // Non-generic datatype - empty type params
                let datatype = (address, module_name, struct_name, vec![]);
                Ok(MoveType::Datatype(datatype))
            }
            SignatureToken::DatatypeInstantiation(instantiation) => {
                let (datatype_handle_idx, type_args) = instantiation.as_ref();
                let datatype_handle = module.datatype_handle_at(*datatype_handle_idx);
                let module_handle = module.module_handle_at(datatype_handle.module);
                let address = *module.address_identifier_at(module_handle.address);
                let module_name = module.identifier_at(module_handle.name).to_string();
                let struct_name = module.identifier_at(datatype_handle.name).to_string();

                // Resolve the type arguments
                let resolved_type_args = type_args
                    .iter()
                    .map(|arg| Self::signature_token_to_move_type(arg, module, type_arguments))
                    .collect::<anyhow::Result<Vec<_>>>()?;

                // Base datatype with empty type params
                let datatype = (address, module_name, struct_name, vec![]);

                // Generic instantiation with resolved type arguments
                Ok(MoveType::DatatypeInstantiation(Box::new((
                    datatype,
                    resolved_type_args,
                ))))
            }
            SignatureToken::Reference(inner) => {
                let inner_type = Self::signature_token_to_move_type(inner, module, type_arguments)?;
                Ok(MoveType::Reference(Box::new(inner_type)))
            }
            SignatureToken::MutableReference(inner) => {
                let inner_type = Self::signature_token_to_move_type(inner, module, type_arguments)?;
                Ok(MoveType::MutableReference(Box::new(inner_type)))
            }
            SignatureToken::TypeParameter(idx) => {
                // Resolve type parameter using the provided type_arguments
                if (*idx as usize) < type_arguments.len() {
                    Ok(type_arguments[*idx as usize].clone())
                } else {
                    // Return TypeParameter variant if not resolved
                    Ok(MoveType::TypeParameter(*idx))
                }
            }
        }
    }
}

// Entry point. Executes a transaction.
// Return all the information that can be used by a client
// to verify execution.
#[allow(clippy::type_complexity)]
pub fn execute_transaction_to_effects(
    txn: ReplayTransaction,
    epoch_store: &dyn EpochStore,
    object_store: &dyn ObjectStore,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> Result<
    (
        Result<(), ExecutionError>, // transaction result
        TxnContextAndEffects,       // data touched and changed during execution
    ),
    anyhow::Error,
> {
    debug!(op = "execute_tx", phase = "start", "execution");
    // TODO: Hook up...
    let config_certificate_deny_set: HashSet<TransactionDigest> = HashSet::new();

    let ReplayTransaction {
        digest,
        checkpoint,
        txn_data,
        effects: expected_effects,
        executor,
        object_cache,
    } = txn;

    let epoch = expected_effects.executed_epoch();
    let _span = debug_span!("execute_tx", %digest, epoch, checkpoint).entered();
    let input_objects = get_input_objects_for_replay(&txn_data, &digest, &object_cache)?;
    let protocol_config = &executor.protocol_config;
    let epoch_data = epoch_store
        .epoch_info(epoch)?
        .ok_or_else(|| anyhow::anyhow!(format!("Epoch {} not found", epoch)))?;
    let epoch_start_timestamp = epoch_data.start_timestamp;
    let gas_status = if txn_data.kind().is_system_tx() {
        SuiGasStatus::new_unmetered()
    } else {
        SuiGasStatus::new(
            txn_data.gas_data().budget,
            txn_data.gas_data().price,
            epoch_data.rgp,
            protocol_config,
        )
        .expect("Failed to create gas status")
    };
    let store: ReplayStore<'_> = ReplayStore {
        checkpoint,
        store: object_store,
        object_cache: RefCell::new(object_cache),
    };
    let input_objects = CheckedInputObjects::new_for_replay(input_objects);
    // TODO(address-balances): Get withdraw status from effects.
    let early_execution_error = get_early_execution_error(
        &digest,
        &input_objects,
        &config_certificate_deny_set,
        // TODO(address-balances): Support balance withdraw status for replay
        &BalanceWithdrawStatus::NoWithdraw,
    );
    let execution_params = match early_execution_error {
        Some(error) => ExecutionOrEarlyError::Err(error),
        None => ExecutionOrEarlyError::Ok(()),
    };
    let (inner_store, gas_status, effects, _execution_timing, result) =
        executor.executor.execute_transaction_to_effects(
            &store,
            protocol_config,
            executor.metrics.clone(),
            false, // expensive checks
            execution_params,
            &epoch,
            epoch_start_timestamp,
            input_objects,
            txn_data.gas_data().clone(),
            gas_status,
            txn_data.kind().clone(),
            txn_data.sender(),
            digest,
            trace_builder_opt,
        );
    let ReplayStore {
        object_cache,
        checkpoint: _,
        store: _,
    } = store;
    let mut object_cache = object_cache.into_inner();

    // Get created objects from transaction effects
    for (object_ref, _owner) in effects.created() {
        let object_id = object_ref.0;
        // Look for the created object in inner_store's written objects
        if let Some(object) = inner_store.written.get(&object_id) {
            object_cache
                .entry(object_id)
                .or_default()
                .insert(object.version().value(), object.clone());
        } else {
            // Return error if created object is not found in inner_store
            return Err(anyhow::anyhow!(
                "Created object {} not found in inner_store written objects",
                object_id
            ));
        }
    }

    debug!(op = "execute_tx", phase = "end", "execution");
    Ok((
        result,
        TxnContextAndEffects {
            txn_data,
            execution_effects: effects,
            expected_effects,
            gas_status,
            object_cache,
            inner_store,
            checkpoint,
            protocol_version: protocol_config.version.as_u64(),
        },
    ))
}

impl ReplayExecutor {
    pub fn new(protocol_config: ProtocolConfig) -> Result<Self, anyhow::Error> {
        let silent = true; // disable Move debug API
        let executor = sui_execution::executor(&protocol_config, silent)
            .context("Filed to create executor. ProtocolConfig inconsistency?")?;

        let registry = prometheus::Registry::new();
        let metrics = Arc::new(LimitsMetrics::new(&registry));

        Ok(Self {
            protocol_config,
            executor,
            metrics,
        })
    }
}

//
// Execution traits implementation for ReplayEnvironment
//

struct ReplayStore<'a> {
    store: &'a dyn ObjectStore,
    object_cache: RefCell<BTreeMap<ObjectID, BTreeMap<u64, Object>>>,
    checkpoint: u64,
}

impl ReplayStore<'_> {
    // utility function shared across traits functions
    fn get_object_at_version(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Option<Object> {
        // look up in the cache
        if let Some(object) = self
            .object_cache
            .borrow()
            .get(object_id)
            .and_then(|versions| versions.get(&version.value()).cloned())
        {
            return Some(object);
        }

        // if not in the cache fetch it from the store
        let object = self
            .store
            .get_objects(&[ObjectKey {
                object_id: *object_id,
                version_query: VersionQuery::Version(version.value()),
            }])
            .map_err(|e| SuiError::Storage(e.to_string()))
            .ok()?
            .into_iter()
            .next()?
            .map(|(obj, _version)| obj);
        // add it to the cache
        if let Some(obj) = &object {
            self.object_cache
                .borrow_mut()
                .entry(obj.id())
                .or_default()
                .insert(obj.version().value(), obj.clone());
        }

        object
    }
}

impl BackingPackageStore for ReplayStore<'_> {
    // Look for a package in the object cache first.
    // If not found, fetch it from the store, add to the cache, and return it.
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        trace!("get_package_object({})", package_id);
        if let Some(versions) = self.object_cache.borrow().get(package_id) {
            debug_assert!(
                versions.len() == 1,
                "Expected only one version in cache for package object {}",
                package_id
            );
            return Ok(Some(PackageObject::new(
                versions.values().next().unwrap().clone(),
            )));
        }
        // If the package is not in the cache, fetch it from the store
        let object_key = ObjectKey {
            object_id: *package_id,
            // Using AtCheckpoint to get the package version at the time of execution
            version_query: VersionQuery::AtCheckpoint(self.checkpoint),
        };
        let package = self
            .store
            .get_objects(&[object_key])
            .map_err(|e| SuiError::Storage(e.to_string()))?;
        debug_assert!(
            package.len() == 1,
            "Expected one package object for {}",
            package_id
        );
        let maybe = package.into_iter().next().and_then(|o| o);
        if let Some((package, _version)) = maybe {
            self.object_cache
                .borrow_mut()
                .entry(*package_id)
                .or_default()
                .insert(package.version().value(), package.clone());
            Ok(Some(PackageObject::new(package)))
        } else {
            Ok(None)
        }
    }
}

impl sui_types::storage::ObjectStore for ReplayStore<'_> {
    // Get an object by its ID. This translates to a query for the object
    // at the checkpoint (mimic latest runtime behavior)
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        trace!("get_object({})", object_id);
        let object = match self.object_cache.borrow().get(object_id) {
            Some(versions) => versions.last_key_value().map(|(_version, obj)| obj.clone()),
            None => {
                let fetched_object = self
                    .store
                    .get_objects(&[ObjectKey {
                        object_id: *object_id,
                        version_query: VersionQuery::AtCheckpoint(self.checkpoint),
                    }])
                    .map_err(|e| SuiError::Storage(e.to_string()))
                    .ok()?
                    .into_iter()
                    .next()?
                    .map(|(obj, _version)| obj)?;

                // Add the fetched object to the cache
                let mut cache = self.object_cache.borrow_mut();
                cache
                    .entry(*object_id)
                    .or_default()
                    .insert(fetched_object.version().value(), fetched_object.clone());

                Some(fetched_object)
            }
        };
        object
    }

    // Get an object by its ID and version
    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        trace!("get_object_by_key({}, {})", object_id, version);
        self.get_object_at_version(object_id, version)
    }
}

impl ChildObjectResolver for ReplayStore<'_> {
    // Load an `Object` at a root version. That is the version that is
    // less than or equal to the given `child_version_upper_bound`.
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        trace!(
            "read_child_object({}, {}, {})",
            _parent,
            child,
            child_version_upper_bound,
        );
        let object_key = ObjectKey {
            object_id: *child,
            version_query: VersionQuery::RootVersion(child_version_upper_bound.value()),
        };
        let object = self
            .store
            .get_objects(&[object_key])
            .map_err(|e| SuiError::Storage(e.to_string()))?;
        debug_assert!(object.len() == 1, "Expected one object for {}", child,);
        let object = object
            .into_iter()
            .next()
            .unwrap()
            .map(|(obj, _version)| obj);

        // Add object to cache if it exists and not already cached
        if let Some(ref obj) = object {
            self.object_cache
                .borrow_mut()
                .entry(obj.id())
                .or_default()
                .insert(obj.version().value(), obj.clone());
        }
        Ok(object)
    }

    // Load a receiving object. Results in a query at a specific version
    // (`receive_object_at_version`).
    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        trace!(
            "get_object_received_at_version({}, {}, {}, {})",
            owner,
            receiving_object_id,
            receive_object_at_version,
            epoch_id
        );
        Ok(self.get_object_at_version(receiving_object_id, receive_object_at_version))
    }
}

//
// unreachable traits
//

impl ParentSync for ReplayStore<'_> {
    fn get_latest_parent_entry_ref_deprecated(&self, object_id: ObjectID) -> Option<ObjectRef> {
        unreachable!(
            "unexpected ParentSync::get_latest_parent_entry_ref_deprecated({})",
            object_id,
        )
    }
}

impl ModuleResolver for ReplayStore<'_> {
    type Error = anyhow::Error;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        unreachable!("unexpected ModuleResolver::get_module({})", id)
    }
}
