// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for ProgrammableTransaction::structural_digest (SIP-70 v2).
//!
//! Verifies that the structural digest is:
//! - Deterministic (same PTB -> same digest)
//! - Version-prefixed (first byte = 0x01, total length = 33)
//! - Sensitive to command addition/removal/reordering
//! - Sensitive to argument changes (different Pure values, different objects)
//! - Sensitive to Result redirection (changing which command output flows where)
//! - Stable when only irrelevant metadata changes (e.g. object digest, object version)
//! - Coin-normalizable (same TypeName + Balance -> same digest regardless of ObjectID)
//! - Wildcard-capable (specified Pure inputs hash as marker, not value)

use crate::base_types::{ObjectDigest, ObjectID, SequenceNumber, random_object_ref};
use crate::transaction::{
    Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, ProgrammableTransaction,
};
use crate::type_input::TypeInput;
use move_core_types::language_storage::{StructTag, TypeTag};
use std::collections::{BTreeMap, BTreeSet};

/// Helper: build a simple MoveCall command
fn make_move_call(
    package: ObjectID,
    module: &str,
    function: &str,
    arguments: Vec<Argument>,
) -> Command {
    Command::MoveCall(Box::new(ProgrammableMoveCall {
        package,
        module: module.to_string(),
        function: function.to_string(),
        type_arguments: vec![],
        arguments,
    }))
}

/// Helper: build a ProgrammableTransaction from inputs and commands
fn make_pt(inputs: Vec<CallArg>, commands: Vec<Command>) -> ProgrammableTransaction {
    ProgrammableTransaction { inputs, commands }
}

/// Helper: build a fake coin TypeTag (Coin<0x2::sui::SUI>)
fn sui_coin_type() -> TypeTag {
    TypeTag::Struct(Box::new(StructTag {
        address: ObjectID::from_hex_literal("0x2").unwrap().into(),
        module: move_core_types::identifier::Identifier::new("sui").unwrap(),
        name: move_core_types::identifier::Identifier::new("SUI").unwrap(),
        type_params: vec![],
    }))
}

/// Helper: build a different coin TypeTag
fn usdc_coin_type() -> TypeTag {
    TypeTag::Struct(Box::new(StructTag {
        address: ObjectID::from_hex_literal("0xdead").unwrap().into(),
        module: move_core_types::identifier::Identifier::new("usdc").unwrap(),
        name: move_core_types::identifier::Identifier::new("USDC").unwrap(),
        type_params: vec![],
    }))
}

// ============================================================================
// Version prefix
// ============================================================================

#[test]
fn test_structural_digest_has_version_prefix() {
    let pt = make_pt(vec![], vec![]);
    let digest = pt.structural_digest();
    // Version prefix 0x01 + 32-byte Blake2b256 hash = 33 bytes
    assert_eq!(digest.len(), 33);
    assert_eq!(digest[0], 0x01);
}

// ============================================================================
// Determinism
// ============================================================================

#[test]
fn test_structural_digest_is_deterministic() {
    let obj_ref = random_object_ref();
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref))],
        vec![make_move_call(pkg, "module", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref))],
        vec![make_move_call(pkg, "module", "func", vec![Argument::Input(0)])],
    );

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Sensitivity to commands
// ============================================================================

#[test]
fn test_digest_changes_with_extra_command() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Pure(bcs::to_bytes(&100u64).unwrap())],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Pure(bcs::to_bytes(&100u64).unwrap())],
        vec![
            make_move_call(pkg, "mod", "func", vec![Argument::Input(0)]),
            make_move_call(pkg, "mod", "func2", vec![Argument::Result(0)]),
        ],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_command_reorder() {
    let pkg = ObjectID::random();

    let cmd_a = make_move_call(pkg, "mod", "func_a", vec![]);
    let cmd_b = make_move_call(pkg, "mod", "func_b", vec![]);

    let pt1 = make_pt(vec![], vec![cmd_a.clone(), cmd_b.clone()]);
    let pt2 = make_pt(vec![], vec![cmd_b, cmd_a]);

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_command_removal() {
    let pkg = ObjectID::random();

    let cmd_a = make_move_call(pkg, "mod", "func_a", vec![]);
    let cmd_b = make_move_call(pkg, "mod", "func_b", vec![]);

    let pt1 = make_pt(vec![], vec![cmd_a.clone(), cmd_b]);
    let pt2 = make_pt(vec![], vec![cmd_a]);

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Sensitivity to arguments
// ============================================================================

#[test]
fn test_digest_changes_with_different_pure_value() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Pure(bcs::to_bytes(&100u64).unwrap())],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Pure(bcs::to_bytes(&200u64).unwrap())],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_different_shared_object() {
    let pkg = ObjectID::random();
    let shared_id_1 = ObjectID::random();
    let shared_id_2 = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: shared_id_1,
            initial_shared_version: SequenceNumber::new(),
            mutability: crate::transaction::SharedObjectMutability::Mutable,
        })],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: shared_id_2,
            initial_shared_version: SequenceNumber::new(),
            mutability: crate::transaction::SharedObjectMutability::Mutable,
        })],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_stable_across_shared_object_version_change() {
    let pkg = ObjectID::random();
    let shared_id = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: shared_id,
            initial_shared_version: SequenceNumber::from_u64(1),
            mutability: crate::transaction::SharedObjectMutability::Mutable,
        })],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: shared_id,
            initial_shared_version: SequenceNumber::from_u64(999),
            mutability: crate::transaction::SharedObjectMutability::Mutable,
        })],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_stable_across_object_digest_change() {
    let pkg = ObjectID::random();
    let obj_id = ObjectID::random();
    let version = SequenceNumber::from_u64(5);

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            obj_id,
            version,
            ObjectDigest::new([0xAA; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            obj_id,
            version,
            ObjectDigest::new([0xBB; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Owned object version stability (Change 2: version dropped)
// ============================================================================

#[test]
fn test_digest_stable_across_owned_object_version_change() {
    // Owned objects are now hashed by ObjectID only — version is dropped because
    // it drifts between proposal vote time and execution time.
    let pkg = ObjectID::random();
    let obj_id = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            obj_id,
            SequenceNumber::from_u64(1),
            ObjectDigest::new([0xAA; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            obj_id,
            SequenceNumber::from_u64(999),
            ObjectDigest::new([0xBB; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_stable_across_receiving_object_version_change() {
    let pkg = ObjectID::random();
    let obj_id = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::Receiving((
            obj_id,
            SequenceNumber::from_u64(1),
            ObjectDigest::new([0xAA; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::Receiving((
            obj_id,
            SequenceNumber::from_u64(999),
            ObjectDigest::new([0xBB; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_different_owned_object() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(random_object_ref()))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(random_object_ref()))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Sensitivity to Result redirection (provenance)
// ============================================================================

#[test]
fn test_digest_changes_with_result_redirection() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![],
        vec![
            make_move_call(pkg, "mod", "produce", vec![]),
            make_move_call(pkg, "mod", "consume", vec![Argument::Result(0)]),
        ],
    );

    let pt2 = make_pt(
        vec![],
        vec![
            make_move_call(pkg, "mod", "produce", vec![]),
            make_move_call(pkg, "mod", "produce2", vec![]),
            make_move_call(pkg, "mod", "consume", vec![Argument::Result(1)]),
        ],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_distinguishes_result_vs_nested_result() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![],
        vec![
            make_move_call(pkg, "mod", "produce", vec![]),
            make_move_call(pkg, "mod", "consume", vec![Argument::Result(0)]),
        ],
    );

    let pt2 = make_pt(
        vec![],
        vec![
            make_move_call(pkg, "mod", "produce", vec![]),
            make_move_call(pkg, "mod", "consume", vec![Argument::NestedResult(0, 0)]),
        ],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// GasCoin normalization
// ============================================================================

#[test]
fn test_digest_gas_coin_is_stable() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::GasCoin])],
    );

    let pt2 = make_pt(
        vec![],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::GasCoin])],
    );

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_gas_coin_differs_from_input() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::GasCoin])],
    );

    let pt2 = make_pt(
        vec![CallArg::Pure(vec![0x00])],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Command type sensitivity
// ============================================================================

#[test]
fn test_digest_distinguishes_command_types() {
    let obj_ref = random_object_ref();

    let pt1 = make_pt(
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref)),
            CallArg::Pure(bcs::to_bytes(&100u64).unwrap()),
        ],
        vec![Command::SplitCoins(Argument::Input(0), vec![Argument::Input(1)])],
    );

    let pt2 = make_pt(
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref)),
            CallArg::Pure(bcs::to_bytes(&100u64).unwrap()),
        ],
        vec![Command::MergeCoins(Argument::Input(0), vec![Argument::Input(1)])],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Target sensitivity
// ============================================================================

#[test]
fn test_digest_changes_with_different_package() {
    let pkg1 = ObjectID::random();
    let pkg2 = ObjectID::random();

    let pt1 = make_pt(vec![], vec![make_move_call(pkg1, "mod", "func", vec![])]);
    let pt2 = make_pt(vec![], vec![make_move_call(pkg2, "mod", "func", vec![])]);

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_different_module() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(vec![], vec![make_move_call(pkg, "mod_a", "func", vec![])]);
    let pt2 = make_pt(vec![], vec![make_move_call(pkg, "mod_b", "func", vec![])]);

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_different_function() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(vec![], vec![make_move_call(pkg, "mod", "func_a", vec![])]);
    let pt2 = make_pt(vec![], vec![make_move_call(pkg, "mod", "func_b", vec![])]);

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Type argument sensitivity
// ============================================================================

#[test]
fn test_digest_changes_with_different_type_arguments() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![],
        vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
            package: pkg,
            module: "mod".to_string(),
            function: "func".to_string(),
            type_arguments: vec![TypeInput::U64],
            arguments: vec![],
        }))],
    );

    let pt2 = make_pt(
        vec![],
        vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
            package: pkg,
            module: "mod".to_string(),
            function: "func".to_string(),
            type_arguments: vec![TypeInput::Bool],
            arguments: vec![],
        }))],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Empty PTB
// ============================================================================

#[test]
fn test_empty_ptb_has_stable_digest() {
    let pt1 = make_pt(vec![], vec![]);
    let pt2 = make_pt(vec![], vec![]);

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Coin normalization (Change 3)
// ============================================================================

#[test]
fn test_coin_normalized_digest_stable_across_different_object_ids() {
    // Two PTBs with different coin ObjectIDs but same TypeName + Balance
    // should produce the same digest when coin_info is provided.
    let pkg = ObjectID::random();
    let coin_id_1 = ObjectID::random();
    let coin_id_2 = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            coin_id_1,
            SequenceNumber::from_u64(1),
            ObjectDigest::new([0xAA; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            coin_id_2,
            SequenceNumber::from_u64(5),
            ObjectDigest::new([0xBB; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    // Without coin_info: different ObjectIDs -> different digests
    assert_ne!(pt1.structural_digest(), pt2.structural_digest());

    // With coin_info (same type + balance): same digest
    let coin_info_1: BTreeMap<usize, (TypeTag, u64)> =
        [(0, (sui_coin_type(), 1000))].into_iter().collect();
    let coin_info_2: BTreeMap<usize, (TypeTag, u64)> =
        [(0, (sui_coin_type(), 1000))].into_iter().collect();

    assert_eq!(
        pt1.structural_digest_with_options(Some(&coin_info_1), &BTreeSet::new()),
        pt2.structural_digest_with_options(Some(&coin_info_2), &BTreeSet::new()),
    );
}

#[test]
fn test_coin_normalized_digest_changes_with_different_balance() {
    let pkg = ObjectID::random();

    let pt = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(random_object_ref()))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let coin_info_1000: BTreeMap<usize, (TypeTag, u64)> =
        [(0, (sui_coin_type(), 1000))].into_iter().collect();
    let coin_info_2000: BTreeMap<usize, (TypeTag, u64)> =
        [(0, (sui_coin_type(), 2000))].into_iter().collect();

    assert_ne!(
        pt.structural_digest_with_options(Some(&coin_info_1000), &BTreeSet::new()),
        pt.structural_digest_with_options(Some(&coin_info_2000), &BTreeSet::new()),
    );
}

#[test]
fn test_coin_normalized_digest_changes_with_different_type() {
    let pkg = ObjectID::random();

    let pt = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(random_object_ref()))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let sui_info: BTreeMap<usize, (TypeTag, u64)> =
        [(0, (sui_coin_type(), 1000))].into_iter().collect();
    let usdc_info: BTreeMap<usize, (TypeTag, u64)> =
        [(0, (usdc_coin_type(), 1000))].into_iter().collect();

    assert_ne!(
        pt.structural_digest_with_options(Some(&sui_info), &BTreeSet::new()),
        pt.structural_digest_with_options(Some(&usdc_info), &BTreeSet::new()),
    );
}

// ============================================================================
// Wildcard slots (Change 4)
// ============================================================================

#[test]
fn test_wildcard_digest_stable_across_different_pure_values() {
    // Two PTBs that differ only in a wildcarded Pure input should produce the same digest.
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![
            CallArg::Pure(bcs::to_bytes(&100u64).unwrap()), // input 0: amount (pinned)
            CallArg::Pure(bcs::to_bytes(&50u64).unwrap()),  // input 1: slippage (wildcard)
        ],
        vec![make_move_call(pkg, "dex", "swap", vec![Argument::Input(0), Argument::Input(1)])],
    );

    let pt2 = make_pt(
        vec![
            CallArg::Pure(bcs::to_bytes(&100u64).unwrap()), // input 0: same amount
            CallArg::Pure(bcs::to_bytes(&999u64).unwrap()), // input 1: different slippage
        ],
        vec![make_move_call(pkg, "dex", "swap", vec![Argument::Input(0), Argument::Input(1)])],
    );

    // Without wildcards: different Pure values -> different digests
    assert_ne!(pt1.structural_digest(), pt2.structural_digest());

    // With wildcard on input 1: same digest
    let wildcards: BTreeSet<u16> = [1].into_iter().collect();
    assert_eq!(
        pt1.structural_digest_with_options(None, &wildcards),
        pt2.structural_digest_with_options(None, &wildcards),
    );

    // Non-wildcarded input (0) still differentiates
    let pt3 = make_pt(
        vec![
            CallArg::Pure(bcs::to_bytes(&200u64).unwrap()), // input 0: different amount
            CallArg::Pure(bcs::to_bytes(&50u64).unwrap()),  // input 1: same slippage
        ],
        vec![make_move_call(pkg, "dex", "swap", vec![Argument::Input(0), Argument::Input(1)])],
    );

    assert_ne!(
        pt1.structural_digest_with_options(None, &wildcards),
        pt3.structural_digest_with_options(None, &wildcards),
    );
}

#[test]
fn test_wildcard_only_applies_to_specified_indices() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![
            CallArg::Pure(bcs::to_bytes(&100u64).unwrap()),
            CallArg::Pure(bcs::to_bytes(&200u64).unwrap()),
        ],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0), Argument::Input(1)])],
    );

    let pt2 = make_pt(
        vec![
            CallArg::Pure(bcs::to_bytes(&100u64).unwrap()),
            CallArg::Pure(bcs::to_bytes(&999u64).unwrap()),
        ],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0), Argument::Input(1)])],
    );

    // Wildcard on input 0 only: input 1 still differentiates
    let wildcards_0: BTreeSet<u16> = [0].into_iter().collect();
    assert_ne!(
        pt1.structural_digest_with_options(None, &wildcards_0),
        pt2.structural_digest_with_options(None, &wildcards_0),
    );

    // Wildcard on input 1 only: input 0 is the same -> same digest
    let wildcards_1: BTreeSet<u16> = [1].into_iter().collect();
    assert_eq!(
        pt1.structural_digest_with_options(None, &wildcards_1),
        pt2.structural_digest_with_options(None, &wildcards_1),
    );
}

#[test]
fn test_wildcard_and_coin_normalization_combined() {
    // Coin normalization and wildcards work together.
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject((
                ObjectID::random(),
                SequenceNumber::from_u64(1),
                ObjectDigest::new([0xAA; 32]),
            ))),
            CallArg::Pure(bcs::to_bytes(&50u64).unwrap()), // slippage
        ],
        vec![make_move_call(pkg, "dex", "swap", vec![Argument::Input(0), Argument::Input(1)])],
    );

    let pt2 = make_pt(
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject((
                ObjectID::random(), // different coin object
                SequenceNumber::from_u64(99),
                ObjectDigest::new([0xBB; 32]),
            ))),
            CallArg::Pure(bcs::to_bytes(&999u64).unwrap()), // different slippage
        ],
        vec![make_move_call(pkg, "dex", "swap", vec![Argument::Input(0), Argument::Input(1)])],
    );

    // Same coin type+balance, wildcard on slippage -> same digest
    let coin_info_1: BTreeMap<usize, (TypeTag, u64)> =
        [(0, (sui_coin_type(), 1000))].into_iter().collect();
    let coin_info_2: BTreeMap<usize, (TypeTag, u64)> =
        [(0, (sui_coin_type(), 1000))].into_iter().collect();
    let wildcards: BTreeSet<u16> = [1].into_iter().collect();

    assert_eq!(
        pt1.structural_digest_with_options(Some(&coin_info_1), &wildcards),
        pt2.structural_digest_with_options(Some(&coin_info_2), &wildcards),
    );
}

// ============================================================================
// Length framing (collision prevention)
// ============================================================================

#[test]
fn test_no_collision_module_function_boundary() {
    // Without length framing, "a" + "bc" hashes same as "ab" + "c".
    // With length framing, they differ.
    let pkg = ObjectID::random();

    let pt1 = make_pt(vec![], vec![make_move_call(pkg, "a", "bc", vec![])]);
    let pt2 = make_pt(vec![], vec![make_move_call(pkg, "ab", "c", vec![])]);

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_no_collision_module_function_boundary_longer() {
    // Another boundary case: "mod" + "func" vs "modf" + "unc"
    let pkg = ObjectID::random();

    let pt1 = make_pt(vec![], vec![make_move_call(pkg, "mod", "func", vec![])]);
    let pt2 = make_pt(vec![], vec![make_move_call(pkg, "modf", "unc", vec![])]);

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_no_collision_different_arg_count() {
    // A single 2-byte Pure vs two 1-byte Pures should differ
    // (list count framing prevents this collision)
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Pure(vec![0x01, 0x02])],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Pure(vec![0x01]), CallArg::Pure(vec![0x02])],
        vec![make_move_call(
            pkg,
            "mod",
            "func",
            vec![Argument::Input(0), Argument::Input(1)],
        )],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Two-mode coin normalization: base vs masked
// ============================================================================

#[test]
fn test_base_digest_uses_object_id_for_coins() {
    // structural_digest() should hash coins by ObjectID (identity-preserving).
    // Different coin ObjectIDs with same type+balance -> different base digests.
    let pkg = ObjectID::random();
    let coin_id_1 = ObjectID::random();
    let coin_id_2 = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            coin_id_1,
            SequenceNumber::from_u64(1),
            ObjectDigest::new([0xAA; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            coin_id_2,
            SequenceNumber::from_u64(1),
            ObjectDigest::new([0xAA; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    // Base digest: different coin IDs -> different digests
    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_masked_digest_normalizes_coins_by_type_and_balance() {
    // structural_digest_with_options(Some(coin_info), ...) normalizes coins.
    // Different ObjectIDs with same type+balance -> same masked digest.
    let pkg = ObjectID::random();
    let coin_id_1 = ObjectID::random();
    let coin_id_2 = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            coin_id_1,
            SequenceNumber::from_u64(1),
            ObjectDigest::new([0xAA; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            coin_id_2,
            SequenceNumber::from_u64(5),
            ObjectDigest::new([0xBB; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let coin_info_1: BTreeMap<usize, (TypeTag, u64)> =
        [(0, (sui_coin_type(), 1000))].into_iter().collect();
    let coin_info_2: BTreeMap<usize, (TypeTag, u64)> =
        [(0, (sui_coin_type(), 1000))].into_iter().collect();

    // Masked with coin normalization: same type+balance -> same digest
    assert_eq!(
        pt1.structural_digest_with_options(Some(&coin_info_1), &BTreeSet::new()),
        pt2.structural_digest_with_options(Some(&coin_info_2), &BTreeSet::new()),
    );

    // But base digest differs (identity-preserving)
    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Complex multi-command PTB (governance scenario)
// ============================================================================

#[test]
fn test_governance_scenario_digest_stability() {
    let pkg = ObjectID::random();
    let coin_ref = random_object_ref();

    let build_ptb = || {
        make_pt(
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_ref)),
                CallArg::Pure(bcs::to_bytes(&1000u64).unwrap()),
            ],
            vec![
                Command::SplitCoins(Argument::Input(0), vec![Argument::Input(1)]),
                make_move_call(pkg, "dex", "swap", vec![Argument::Result(0)]),
                make_move_call(pkg, "vault", "deposit", vec![Argument::Result(1)]),
            ],
        )
    };

    let pt1 = build_ptb();
    let pt2 = build_ptb();

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_governance_scenario_detects_swap_target_change() {
    let pkg1 = ObjectID::random();
    let pkg2 = ObjectID::random();
    let vault_pkg = ObjectID::random();
    let coin_ref = random_object_ref();

    let pt1 = make_pt(
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_ref)),
            CallArg::Pure(bcs::to_bytes(&1000u64).unwrap()),
        ],
        vec![
            Command::SplitCoins(Argument::Input(0), vec![Argument::Input(1)]),
            make_move_call(pkg1, "dex", "swap", vec![Argument::Result(0)]),
            make_move_call(vault_pkg, "vault", "deposit", vec![Argument::Result(1)]),
        ],
    );

    let pt2 = make_pt(
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_ref)),
            CallArg::Pure(bcs::to_bytes(&1000u64).unwrap()),
        ],
        vec![
            Command::SplitCoins(Argument::Input(0), vec![Argument::Input(1)]),
            make_move_call(pkg2, "dex", "swap", vec![Argument::Result(0)]),
            make_move_call(vault_pkg, "vault", "deposit", vec![Argument::Result(1)]),
        ],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}
