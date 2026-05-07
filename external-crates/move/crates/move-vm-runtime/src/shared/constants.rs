// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Constants
// -------------------------------------------------------------------------------------------------

pub const OPERAND_STACK_SIZE_LIMIT: usize = 1024;
pub const CALL_STACK_SIZE_LIMIT: usize = 1024;

/// Maximal number of locals any individual call can have.
pub const LOCALS_PER_FRAME_LIMIT: usize = 2_048;

/// Maximum type depth when applying a type substitution.
pub const TYPE_DEPTH_MAX: u64 = 256;

/// Maximal depth of a value in terms of type depth.
pub const VALUE_DEPTH_MAX: u64 = 128;

/// Maximal nodes which are allowed when converting to layout. This includes the types of
/// fields for struct types.
/// Maximal nodes which are allowed when converting to layout. This includes the types of
/// fields for datatypes.
pub const HISTORICAL_MAX_TYPE_TO_LAYOUT_NODES: u64 = 256;

/// Maximal nodes which are all allowed when instantiating a generic type. This does not include
/// field types of datatypes.
pub const MAX_TYPE_INSTANTIATION_NODES: u64 = 128;

/// Size of the type depth LRU
/// TODO(vm-rewrite): find a good bound for this
pub const TYPE_DEPTH_LRU_SIZE: usize = 16_384;

/// Size of the linkage-cahge virtual dispatch LRU
/// TODO(vm-rewrite): find a good bound for this
/// This number is currently 1 GB / 128 bytes (size of VMDispatchTables), giving approximately
/// a gigabytes of storage to VTables (though this disregards key and LRU overhead).
pub const VIRTUAL_DISPATCH_TABLE_CACHE_SIZE: usize = 1_000_000;

/// Maximum number of identifiers we can ever intern.
/// TODO Set to 10 billion, but should be experimentally determined based on actual run data.
pub const IDENTIFIER_INTERNER_SIZE_LIMIT: usize = 10_000_000_000;

// -------------------------------------------------------------------------------------------------
// Profiling Env Vars
// -------------------------------------------------------------------------------------------------

/// Env var naming a path that the bytecode profiler writes JSON to when
/// `BytecodeSnapshot::maybe_dump_to_env_file` or an equivalent hook is
/// invoked. Unset = no file dump.
pub const MOVE_VM_DUMP_PROFILE_FILE_ENV: &str = "MOVE_VM_DUMP_PROFILE_FILE";

/// Env var controlling the replay-time bytecode-profile dumping policy
/// (see `sui_replay_2::profiling::BytecodeProfileMode`).
pub const MOVE_VM_PROFILE_MODE_ENV: &str = "MOVE_VM_PROFILE_MODE";
