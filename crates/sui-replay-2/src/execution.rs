// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    errors::ReplayError,
    replay_interface::{EpochStore, ObjectStore},
    replay_txn::ReplayTransaction,
};
use move_binary_format::CompiledModule;
use move_bytecode_source_map::utils::serialize_to_json_string;
use move_command_line_common::files::MOVE_BYTECODE_EXTENSION;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;
use move_trace_format::format::MoveTraceBuilder;
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use sui_execution::Executor;
use sui_types::digests::TransactionDigest;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, VersionNumber},
    committee::EpochId,
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::{ExecutionError, SuiResult},
    gas::SuiGasStatus,
    metrics::LimitsMetrics,
    object::{Data, Object},
    storage::{BackingPackageStore, ChildObjectResolver, PackageObject, ParentSync},
    supported_protocol_versions::ProtocolConfig,
    transaction::{CheckedInputObjects, TransactionDataAPI},
};

const DEFAULT_TRACE_OUTPUT_DIR: &str = "replay";

const TRACE_FILE_NAME: &str = "trace.json.zst";

const BCODE_DIR: &str = "bytecode";

const SOURCE_DIR: &str = "source";

// Executor for the replay. Created and used by `ReplayTransaction`.
pub struct ReplayExecutor {
    protocol_config: ProtocolConfig,
    executor: Arc<dyn Executor + Send + Sync>,
    metrics: Arc<LimitsMetrics>,
}

pub fn execute_transaction_to_effects(
    txn: ReplayTransaction,
    epoch_store: &dyn EpochStore,
    object_store: &dyn ObjectStore,
    trace_execution: Option<Option<String>>,
) -> Result<
    (
        Result<(), ExecutionError>,
        TransactionEffects,
        SuiGasStatus,
        TransactionEffects,
    ),
    ReplayError,
> {
    // TODO: Hook up...
    let certificate_deny_set = HashSet::new();

    let ReplayTransaction {
        digest,
        txn_data,
        effects: expected_effects,
        executor,
        input_objects,
        objects: _,
    } = txn;

    let protocol_config = &executor.protocol_config;
    let epoch = expected_effects.executed_epoch();
    let epoch_data = epoch_store.epoch_info(epoch)?;
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
        epoch,
        store: object_store,
    };
    let mut trace_builder_opt = if trace_execution.is_some() {
        Some(MoveTraceBuilder::new())
    } else {
        None
    };
    let (_inner_store, gas_status, effects, _execution_timing, result) =
        executor.executor.execute_transaction_to_effects(
            &store,
            protocol_config,
            executor.metrics.clone(),
            false, // expensive checks
            &certificate_deny_set,
            &epoch,
            epoch_start_timestamp,
            CheckedInputObjects::new_for_replay(input_objects),
            txn_data.gas_data().clone(),
            gas_status,
            txn_data.kind().clone(),
            txn_data.sender(),
            digest,
            &mut trace_builder_opt,
        );

    if let Some(trace_builder) = trace_builder_opt {
        // unwrap is safe if trace_builder_opt.is_some() holds
        let output_path = get_trace_output_path(trace_execution.unwrap())?;
        save_trace_output(&output_path, digest, trace_builder, &[])?;
    }
    Ok((result, effects, gas_status, expected_effects))
}

/// Gets the path to store trace output (either the default one './replay' or user-specified).
/// Upon success, the path will exist in the file system.
fn get_trace_output_path(trace_execution: Option<String>) -> Result<PathBuf, ReplayError> {
    match trace_execution {
        Some(p) => {
            let path = PathBuf::from(p);
            if !path.exists() {
                return Err(ReplayError::TracingError {
                    err: format!(
                        "User-specified path to store trace output does not exist: {:?}",
                        path
                    ),
                });
            }
            if !path.is_dir() {
                return Err(ReplayError::TracingError {
                    err: format!(
                        "User-specified path to store trace output is not a directory: {:?}",
                        path
                    ),
                });
            }
            Ok(path)
        }
        None => {
            let current_dir = env::current_dir().map_err(|e| ReplayError::TracingError {
                err: format!("Failed to get current directory: {:?}", e),
            })?;
            let path = current_dir.join(DEFAULT_TRACE_OUTPUT_DIR);
            if path.exists() && path.is_file() {
                return Err(ReplayError::TracingError {
                    err: format!(
                        "Default path to store trace output already exists and is a file, not a directory: {:?}",
                        path
                    ),
                });
            }
            fs::create_dir_all(&path).map_err(|e| ReplayError::TracingError {
                err: format!("Failed to create default trace output directory: {:?}", e),
            })?;
            Ok(path)
        }
    }
}

/// Saves the trace and additional metadata needed to analyze the trace
/// to a subderectory named after the transaction digest.
fn save_trace_output(
    output_path: &Path,
    digest: TransactionDigest,
    trace_builder: MoveTraceBuilder,
    pkgs: &[Object],
) -> Result<(), ReplayError> {
    let txn_output_path = output_path.join(digest.to_string());
    if txn_output_path.exists() {
        return Err(ReplayError::TracingError {
            err: format!(
                "Trace output directory for transaction {} already exists: {:?}",
                digest, txn_output_path
            ),
        });
    }
    fs::create_dir_all(&txn_output_path).map_err(|e| ReplayError::TracingError {
        err: format!(
            "Failed to create trace output directory for transaction {}: {:?}",
            digest, e
        ),
    })?;
    let trace = trace_builder.into_trace();
    let json = trace.into_compressed_json_bytes();
    let trace_file_path = txn_output_path.join(TRACE_FILE_NAME);
    fs::write(&trace_file_path, json).map_err(|e| ReplayError::TracingError {
        err: format!(
            "Failed to write trace output to {:?}: {:?}",
            trace_file_path, e
        ),
    })?;
    let bcode_dir = txn_output_path.join(BCODE_DIR);
    fs::create_dir(&bcode_dir).map_err(|e| ReplayError::TracingError {
        err: format!(
            "Failed to create bytecode output directory '{:?}': {:?}",
            bcode_dir, e
        ),
    })?;
    for obj in pkgs.iter() {
        if let Data::Package(pkg) = &obj.data {
            let pkg_addr = format!("{:?}", obj.id());
            let bcode_pkg_dir = bcode_dir.join(&pkg_addr);
            fs::create_dir(&bcode_pkg_dir).map_err(|e| ReplayError::TracingError {
                err: format!("Failed to create bytecode package directory: {:?}", e),
            })?;
            for (mod_name, serialized_mod) in pkg.serialized_module_map() {
                let compiled_mod = CompiledModule::deserialize_with_defaults(serialized_mod)
                    .map_err(|e| ReplayError::TracingError {
                        err: format!(
                            "Failed to deserialize module {:?} in package {}: {:?}",
                            mod_name, &pkg_addr, e
                        ),
                    })?;
                let d = Disassembler::from_module(&compiled_mod, Spanned::unsafe_no_loc(()).loc)
                    .map_err(|e| ReplayError::TracingError {
                        err: format!(
                            "Failed to create disassembler for module {:?} in package {}: {:?}",
                            mod_name, &pkg_addr, e
                        ),
                    })?;
                let (disassemble_string, bcode_map) =
                    d.disassemble_with_source_map()
                        .map_err(|e| ReplayError::TracingError {
                            err: format!(
                                "Failed to disassemble module {:?} in package {}: {:?}",
                                mod_name, &pkg_addr, e
                            ),
                        })?;
                let bcode_map_json = serialize_to_json_string(&bcode_map).map_err(|e| {
                    ReplayError::TracingError {
                        err: format!(
                            "Failed to serialize bytecode source map for module {:?} in package {}: {:?}",
                            mod_name, &pkg_addr, e
                        ),
                    }
                })?;
                fs::write(
                    bcode_pkg_dir.join(format!("{}.{}", mod_name, MOVE_BYTECODE_EXTENSION)),
                    disassemble_string,
                )
                .map_err(|e| ReplayError::TracingError {
                    err: format!(
                        "Failed to write disassembled bytecode for module {:?} in package {}: {:?}",
                        mod_name, &pkg_addr, e
                    ),
                })?;
                fs::write(
                    bcode_pkg_dir.join(format!("{}.json", mod_name)),
                    bcode_map_json,
                )
                .map_err(|e| ReplayError::TracingError {
                    err: format!(
                        "Failed to write bytecode source map for module {:?} in package {}: {:?}",
                        mod_name, &pkg_addr, e
                    ),
                })?;
            }
        }
    }
    // create empty sources directory as a known placeholder for the users
    // to put optional source files there
    let src_dir = txn_output_path.join(SOURCE_DIR);
    fs::create_dir(&src_dir).map_err(|e| ReplayError::TracingError {
        err: format!(
            "Failed to create source output directory '{:?}': {:?}",
            src_dir, e
        ),
    })?;

    Ok(())
}

impl ReplayExecutor {
    pub fn new(
        protocol_config: ProtocolConfig,
        enable_profiler: Option<PathBuf>,
    ) -> Result<Self, ReplayError> {
        let silent = true; // disable Move debug API
        let executor =
            sui_execution::executor(&protocol_config, silent, enable_profiler).map_err(|e| {
                ReplayError::ExecutorError {
                    err: format!("{:?}", e),
                }
            })?;

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
    epoch: u64,
}

impl BackingPackageStore for ReplayStore<'_> {
    // must resolve locally
    fn get_package_object(&self, _package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        panic!("TBI: get_package_object");
    }
}

impl sui_types::storage::ObjectStore for ReplayStore<'_> {
    // must resolve locally
    fn get_object(&self, _object_id: &ObjectID) -> Option<Object> {
        panic!("TBI: get_package_object");
    }

    // must resolve locally?
    fn get_object_by_key(&self, _object_id: &ObjectID, _version: VersionNumber) -> Option<Object> {
        panic!("TBI: get_package_object");
    }
}

impl ChildObjectResolver for ReplayStore<'_> {
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        _child: &ObjectID,
        _child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        panic!("TBI: get_package_object");
    }

    // must resolve locally?
    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        _receiving_object_id: &ObjectID,
        _receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        panic!("TBI: get_package_object");
    }
}

impl ParentSync for ReplayStore<'_> {
    fn get_latest_parent_entry_ref_deprecated(&self, object_id: ObjectID) -> Option<ObjectRef> {
        unreachable!(
            "unexpected ParentSync::get_latest_parent_entry_ref_deprecated({})",
            object_id,
        )
    }
}

impl ModuleResolver for ReplayStore<'_> {
    type Error = ReplayError;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        unreachable!("unexpected ModuleResolver::get_module({})", id)
    }
}
