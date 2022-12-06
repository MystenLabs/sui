// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Constants that change the behavior of the protocol

// ==== Move VM, Move bytecode verifier, and execution limits ===

/// Maximum Move bytecode version the VM understands. All older versions are accepted.
pub const MOVE_BINARY_FORMAT_VERSION: u32 = 6;

/// Maximum size of the `contents` part of an object, in bytes. Enforced by the Sui adapter when effects are produced.
pub const MAX_MOVE_OBJECT_SIZE: u64 = 250 * 1024; // 250 KB

// TODO: increase to 500 KB. currently, publishing a package > 500 KB exceeds the max computation gas cost
/// Maximum size of a Move package object, in bytes. Enforced by the Sui adapter at the end of a publish transaction.
pub const MAX_MOVE_PACKAGE_SIZE: u64 = 100 * 1024; // 100 KB

/// Maximum number of gas units that a single MoveCall transaction can use. Enforced by the Sui adapter.
pub const MAX_TX_GAS: u64 = 1_000_000_000;

/// Maximum number of nested loops. Enforced by the Move bytecode verifier.
pub const MAX_LOOP_DEPTH: usize = 5;

/// Maximum number of type arguments that can be bound to generic type parameters. Enforced by the Move bytecode verifier.
pub const MAX_GENERIC_INSTANTIATION_LENGTH: usize = 32;

/// Maximum number of paramters that a Move function can have. Enforced by the Move bytecode verifier.
pub const MAX_FUNCTION_PARAMETERS: usize = 128;

/// Maximum number of basic blocks that a Move function can have. Enforced by the Move bytecode verifier.
pub const MAX_BASIC_BLOCKS: usize = 1024;

/// Maximum stack size value. Enforced by the Move bytecode verifier.
pub const MAX_VALUE_STACK_SIZE: usize = 1024;

/// Maximum number of type nodes. Enforced by the Move bytecode verifier.
pub const MAX_TYPE_NODES: usize = 256;

/// Maximum number of pushes in one function. Enforced by the Move bytecode verifier.
pub const MAX_PUSH_SIZE: usize = 10000;

/// Maximum dependency depth. Enforced by the Move bytecode verifier.
pub const MAX_DEPENDENCY_DEPTH: u64 = 100;

/// Maximum number of events that a single Move function can emit. Enforced by the Sui adapter during execution.
// TODO: is this per Move function, or per transaction? And if per-function, can't I get around the limit by calling
// a function that emits 255 events in a loop?
pub const MAX_NUM_EVENT_EMIT: u64 = 256;

// === Execution gas costs ====
// note: per-instruction and native function gas costs live in the sui-cost-tables crate

/// Base cost for any Sui transaction
pub const BASE_TX_COST_FIXED: u64 = 10_000;

/// Additional cost for Move call transactions that use a shared object.
/// i.e., the base cost of such a transaction is BASE_TX_COST_FIXED + CONSENSUS_COST
pub const CONSENSUS_COST: u64 = 100_000;

/// Cost per byte of a Move call transaction
/// i.e., the cost of such a transaction is base_cost + (BASE_TX_COST_PER_BYTE * size)
pub const BASE_TX_COST_PER_BYTE: u64 = 0;

/// Additional cost for a transaction that publishes a package
/// i.e., the base cost of such a transaction is BASE_TX_COST_FIXED + PACKAGE_PUBLISH_COST_FIXED
pub const PACKAGE_PUBLISH_COST_FIXED: u64 = 1_000;

/// Cost per byte for a transaction that publishes a package
pub const PACKAGE_PUBLISH_COST_PER_BYTE: u64 = 80;

// Per-byte cost of reading an object during transaction execution
pub const OBJ_ACCESS_COST_READ_PER_BYTE: u64 = 15;

// Per-byte cost of writing an object during transaction execution
pub const OBJ_ACCESS_COST_MUTATE_PER_BYTE: u64 = 40;

// Per-byte cost of deleting an object during transaction execution
pub const OBJ_ACCESS_COST_DELETE_PER_BYTE: u64 = 40;

/// Per-byte cost charged for each input object to a transaction.
/// Meant to approximate the cost of checking locks for each object
// TODO: I'm not sure that this cost makes sense. Checking locks is "free"
// in the sense that an invalid tx that can never be committed/pay gas can
// force validators to check an abitrary number of locks. If those checks are
// "free" for invalid transactions, why charge for them in valid transactions
// TODO: if we keep this, I think we probably want it to be a fixed cost rather
// than a per-byte cost. checking an object lock should not require loading an
// entire object, just consulting an ID -> tx digest map
pub const OBJ_ACCESS_COST_VERIFY_PER_BYTE: u64 = 200;

/// === Storage gas costs ===

/// Per-byte cost of storing an object in the Sui global object store. Some of this cost may be refundable if the object is later freed
pub const OBJ_DATA_COST_REFUNDABLE: u64 = 100;

// Per-byte cost of storing an object in the Sui transaction log (e.g., in CertifiedTransactionEffects)
// This depends on the size of various fields including the effects
// TODO: I don't fully understand this^ and more details would be useful
pub const OBJ_METADATA_COST_NON_REFUNDABLE: u64 = 50;
